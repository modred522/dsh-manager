//! dsh 进程探测。
//!
//! Electron 版靠两个外部进程：`spawn netstat -ano` 解析文本拿监听 PID，
//! 再 `spawn powershell -File find-dsh.ps1` 走 WMI 拿命令行/内存/CPU。
//! 这里换成进程内调用：
//!   * 监听端口 -> `netstat2`（IPv4 + IPv6 都查，dsh 是 Node 默认双栈监听，
//!     只查 IPv4 会漏掉 `[::]:3080` 那一行，导致误判"DSH 未运行"）
//!   * 进程详情 -> `sysinfo`（自带 CPU 采样，不用像原来那样手算 100ns 差值）
//!
//! 于是 `find-dsh.ps1` 可以删掉，GOTCHAS 第一节那堆「.ps1 必须纯 ASCII」的坑一并消失。

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use netstat2::{AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState};
use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// 详情缓存时长，与 Electron 版的 30 秒一致（sysinfo 的 CPU% 也靠两次刷新的间隔）。
const DETAIL_CACHE: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DshProcess {
    pub pid: u32,
    pub listening: bool,
    pub name: String,
    pub command_line: String,
    pub start_time: Option<String>,
    pub mem_mb: u64,
    pub cpu_percent: Option<f32>,
}

/// 找出监听指定端口的 PID 集合。
pub fn listening_pids(port: u16) -> HashSet<u32> {
    let af = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let mut pids = HashSet::new();
    let Ok(sockets) = netstat2::get_sockets_info(af, ProtocolFlags::TCP) else {
        return pids;
    };
    for si in sockets {
        if let ProtocolSocketInfo::Tcp(tcp) = &si.protocol_socket_info {
            if tcp.state == TcpState::Listen && tcp.local_port == port {
                pids.extend(si.associated_pids.iter().copied().filter(|p| *p > 0));
            }
        }
    }
    pids
}

/// sysinfo 实例常驻：CPU% 需要两次刷新之间的差值，每次新建拿不到有效值。
struct DetailCache {
    system: System,
    fetched_at: Option<Instant>,
    procs: Vec<DshProcess>,
}

static CACHE: Mutex<Option<DetailCache>> = Mutex::new(None);

/// 汇总：监听端口（每次都查，很轻）+ 进程详情（30 秒缓存）。
pub fn dsh_processes(port: u16) -> Vec<DshProcess> {
    let listening = listening_pids(port);

    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(|| DetailCache {
        system: System::new(),
        fetched_at: None,
        procs: Vec::new(),
    });

    let stale = cache
        .fetched_at
        .map(|t| t.elapsed() >= DETAIL_CACHE)
        .unwrap_or(true);
    if stale {
        cache.procs = fetch_details(&mut cache.system, &listening);
        cache.fetched_at = Some(Instant::now());
    }

    // 详情可能过期（缓存期内新起的进程还没详情），监听集合才是当下的事实。
    let mut out: Vec<DshProcess> = Vec::new();
    for p in &cache.procs {
        if listening.contains(&p.pid) {
            let mut p = p.clone();
            p.listening = true;
            out.push(p);
        }
    }
    let known: HashSet<u32> = out.iter().map(|p| p.pid).collect();
    for pid in &listening {
        if !known.contains(pid) {
            out.push(DshProcess {
                pid: *pid,
                listening: true,
                name: String::new(),
                command_line: String::new(),
                start_time: None,
                mem_mb: 0,
                cpu_percent: None,
            });
        }
    }
    out.sort_by_key(|p| p.pid);
    out
}

fn fetch_details(system: &mut System, pids: &HashSet<u32>) -> Vec<DshProcess> {
    if pids.is_empty() {
        return Vec::new();
    }
    let targets: Vec<Pid> = pids.iter().map(|p| Pid::from_u32(*p)).collect();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&targets),
        true,
        ProcessRefreshKind::nothing()
            .with_cmd(sysinfo::UpdateKind::Always)
            .with_memory()
            .with_cpu(),
    );

    let mut out = Vec::new();
    for pid in pids {
        let Some(proc_) = system.process(Pid::from_u32(*pid)) else {
            continue;
        };
        // npm shim 用 %dp0%\ 拼接，命令行里会出现双反斜杠（正常现象，显示前归一化）。
        let command_line = proc_
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ")
            .replace("\\\\", "\\");
        let cpu = proc_.cpu_usage();
        out.push(DshProcess {
            pid: *pid,
            listening: true,
            name: proc_.name().to_string_lossy().into_owned(),
            command_line,
            start_time: format_start_time(proc_.start_time()),
            mem_mb: proc_.memory() / 1_048_576,
            // 首次刷新拿不到有意义的 CPU%，报 None 而不是假的 0。
            cpu_percent: if cpu > 0.0 {
                Some((cpu * 10.0).round() / 10.0)
            } else {
                None
            },
        });
    }
    out
}

/// sysinfo 给的是 unix 秒；格式化成与原来 WMI 路径一致的 `YYYY-MM-DD HH:MM:SS`。
fn format_start_time(secs: u64) -> Option<String> {
    if secs == 0 {
        return None;
    }
    let dt = chrono::DateTime::from_timestamp(secs as i64, 0)?;
    Some(
        dt.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
    )
}

/// 停止指定 PID（空/None 表示全停）。返回实际下手的进程数。
///
/// 沿用 `taskkill /pid <n> /t /f`：dsh 会拉起子进程（node-pty 等），
/// 必须带 `/t` 连子树一起收掉，否则端口不会释放。
pub fn stop_processes(port: u16, pids: Option<&[u32]>) -> usize {
    let all = dsh_processes(port);
    let targets: Vec<u32> = match pids {
        Some(list) if !list.is_empty() => all
            .iter()
            .map(|p| p.pid)
            .filter(|pid| list.contains(pid))
            .collect(),
        _ => all.iter().map(|p| p.pid).collect(),
    };
    if targets.is_empty() {
        return 0;
    }

    let mut args: Vec<String> = Vec::new();
    for pid in &targets {
        args.push("/pid".into());
        args.push(pid.to_string());
    }
    args.push("/t".into());
    args.push("/f".into());

    let _ = std::process::Command::new("taskkill").args(&args).output();

    // 强制下次重新拉详情。
    if let Ok(mut guard) = CACHE.lock() {
        if let Some(c) = guard.as_mut() {
            c.fetched_at = None;
            c.procs.clear();
        }
    }
    targets.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listening_pids_runs_and_is_consistent() {
        // 不假设本机在监听什么，只确认调用不 panic 且 PID 合法。
        let pids = listening_pids(3080);
        for pid in &pids {
            assert!(*pid > 0);
        }
    }

    #[test]
    fn unused_port_has_no_listener() {
        // 65000 上几乎不可能有监听；主要是确认过滤逻辑不会把无关 socket 算进来。
        assert!(listening_pids(65000).is_empty());
    }

    #[test]
    fn dsh_processes_shape_is_sorted_and_listening() {
        let procs = dsh_processes(3080);
        let mut sorted = procs.clone();
        sorted.sort_by_key(|p| p.pid);
        assert_eq!(
            procs.iter().map(|p| p.pid).collect::<Vec<_>>(),
            sorted.iter().map(|p| p.pid).collect::<Vec<_>>()
        );
        assert!(procs.iter().all(|p| p.listening));
    }

    #[test]
    fn start_time_formatting() {
        assert_eq!(format_start_time(0), None);
        let s = format_start_time(1_600_000_000).expect("应格式化出时间");
        assert_eq!(s.len(), 19, "形如 YYYY-MM-DD HH:MM:SS, 实际 {s}");
        assert!(s.starts_with("20"));
    }
}
