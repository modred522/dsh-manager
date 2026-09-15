//! 日志：按天滚动写盘（保留 7 天）+ 广播到所有窗口。
//!
//! 移植自 Electron 版 `main.js` 的日志区，并带上 v1.0.6 热修的三项行为：
//!   1. 落盘/广播前走 `redact_secrets()`（`dsh web` 会把带 token 的地址打到 stdout）
//!   2. 子进程输出走 `log_child_output()`：拆行 + 连续重复折叠 + 每分钟条数上限
//!   3. `cleanup_old_logs()` 除启动时外还挂周期任务（托盘常驻应用可能连跑数周）
//!
//! `AppHandle` 存全局：日志调用点遍布各模块，逐层传句柄不值得。应用本身是单实例，
//! 这与 Electron 版把 `mainWindow` 放模块全局是同一个取舍。

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};

use crate::config;
use crate::pure::{redact_secrets, split_log_chunk};

static APP: OnceLock<AppHandle> = OnceLock::new();

/// 在 `setup()` 里调用一次。之后 `log()` 才能广播。
pub fn init(app: AppHandle) {
    let _ = APP.set(app);
}

fn log_file_path() -> std::path::PathBuf {
    let now = chrono::Local::now();
    config::log_dir().join(format!("dsh-manager-{}.log", now.format("%Y-%m-%d")))
}

/// 写一条日志：带时间戳、打码、落盘、广播到所有窗口。
///
/// 日志正文保持中文（技术输出，不走 i18n），与 Electron 版一致。
pub fn log(line: impl AsRef<str>) {
    let text = format!(
        "[{}] {}",
        chrono::Local::now().format("%H:%M:%S"),
        redact_secrets(line.as_ref())
    );

    // 写盘失败不致命（磁盘满、目录被占用都不该让功能停摆）。
    if std::fs::create_dir_all(config::log_dir()).is_ok() {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file_path())
        {
            let _ = writeln!(f, "{text}");
        }
    }

    if let Some(app) = APP.get() {
        let _ = app.emit("log", &text);
    }
}

/// 启动后回填用的历史日志（当天文件的最后 300 行）。
pub fn recent_log_lines() -> Vec<String> {
    match std::fs::read_to_string(log_file_path()) {
        Ok(text) => {
            let lines: Vec<String> = text
                .lines()
                .filter(|l| !l.is_empty())
                .map(|s| s.to_string())
                .collect();
            let start = lines.len().saturating_sub(300);
            lines[start..].to_vec()
        }
        Err(_) => Vec::new(),
    }
}

/// 删掉 7 天前的日志文件。
///
/// 注意这个函数必须被**周期调用**，不能只在启动时跑一次：托盘常驻应用可能连跑
/// 数周，只在启动清一次等于保留策略失效（Electron 版实测留下过 11 天前的文件）。
pub fn cleanup_old_logs() {
    const WEEK: Duration = Duration::from_secs(7 * 86400);
    let Ok(entries) = std::fs::read_dir(config::log_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if modified.elapsed().map(|age| age > WEEK).unwrap_or(false) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

// ---------------------------------------------------------------------------
// 子进程输出节流
// ---------------------------------------------------------------------------

const CHILD_REPEAT_FLUSH: Duration = Duration::from_secs(5);
const CHILD_RATE_WINDOW: Duration = Duration::from_secs(60);
const CHILD_RATE_MAX: u32 = 200;

#[derive(Default)]
struct ChildThrottle {
    last_line: Option<String>,
    repeat: u32,
    repeat_since: Option<Instant>,
    window_start: Option<Instant>,
    written: u32,
    suppressed: u32,
}

static THROTTLE: Mutex<Option<ChildThrottle>> = Mutex::new(None);

/// 子进程 stdout/stderr 专用入口。
///
/// 直接把 chunk 丢给 `log()` 有两个问题：chunk 自带结尾换行会让每条之间多一个空行；
/// 出问题的插件会疯狂刷同一行（实测某插件余额接口 401，两分钟 60+ 条），把真正的消息冲走。
pub fn log_child_output(chunk: &str) {
    for line in split_log_chunk(chunk) {
        // 先在锁内算出该写哪几条，再到锁外真正写（log() 要写盘+广播，不该持锁做）。
        let decision = {
            let mut guard = THROTTLE.lock().unwrap_or_else(|e| e.into_inner());
            let t = guard.get_or_insert_with(ChildThrottle::default);
            decide(t, &line)
        };
        for l in decision.into_lines() {
            log(l);
        }
    }
}

/// 一行输入产生的日志动作。`decide()` 不做任何 I/O，因此可以直接被单测覆盖，
/// 测到的就是生产路径本身。
#[derive(Debug, Default, PartialEq)]
struct Decision {
    /// 需要补发的"上一行重复 N 次"。
    repeat_summary: Option<u32>,
    /// 需要补发的"上一分钟另有 N 条被限频抑制"。
    suppressed_summary: Option<u32>,
    /// 本行是否真的写出去。
    line: Option<String>,
}

impl Decision {
    /// 输出顺序：重复汇总 → 限频汇总 → 本行（与 Electron 版一致）。
    fn into_lines(self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(n) = self.repeat_summary {
            out.push(format!("↑ 上一行重复 {n} 次（已折叠）"));
        }
        if let Some(n) = self.suppressed_summary {
            out.push(format!("（上一分钟另有 {n} 条子进程输出被限频抑制）"));
        }
        if let Some(l) = self.line {
            out.push(l);
        }
        out
    }
}

fn decide(t: &mut ChildThrottle, line: &str) -> Decision {
    // 连续重复：只计数，等下一条不同的行（或攒够 5 秒）再汇总。
    if t.last_line.as_deref() == Some(line) {
        t.repeat += 1;
        let due = t
            .repeat_since
            .map(|since| since.elapsed() >= CHILD_REPEAT_FLUSH)
            .unwrap_or(false);
        if due {
            t.repeat_since = Some(Instant::now());
            return Decision {
                repeat_summary: Some(std::mem::take(&mut t.repeat)),
                ..Default::default()
            };
        }
        if t.repeat_since.is_none() {
            t.repeat_since = Some(Instant::now());
        }
        return Decision::default();
    }

    let pending_repeat = std::mem::take(&mut t.repeat);
    let repeat_summary = (pending_repeat > 0).then_some(pending_repeat);
    t.repeat_since = None;
    t.last_line = Some(line.to_string());

    // 限频窗口翻滚。
    let now = Instant::now();
    let rolled = t
        .window_start
        .map(|s| now.duration_since(s) > CHILD_RATE_WINDOW)
        .unwrap_or(true);
    let mut suppressed_summary = None;
    if rolled {
        if t.suppressed > 0 {
            suppressed_summary = Some(t.suppressed);
        }
        t.window_start = Some(now);
        t.written = 0;
        t.suppressed = 0;
    }

    if t.written >= CHILD_RATE_MAX {
        t.suppressed += 1;
        // 这行没真正写出去，别让后续的"重复 N 次"指向它。
        t.last_line = None;
        return Decision {
            repeat_summary,
            suppressed_summary,
            line: None,
        };
    }

    t.written += 1;
    Decision {
        repeat_summary,
        suppressed_summary,
        line: Some(line.to_string()),
    }
}

/// 退出前把攒着的重复计数吐出来，别丢信息。
pub fn flush_child_repeat() {
    let pending = {
        let mut guard = THROTTLE.lock().unwrap_or_else(|e| e.into_inner());
        match guard.as_mut() {
            Some(t) => std::mem::take(&mut t.repeat),
            None => 0,
        }
    };
    if pending > 0 {
        log(format!("↑ 上一行重复 {pending} 次（已折叠）"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 直接驱动决策函数，绕开写盘与广播。
    fn run(t: &mut ChildThrottle, chunk: &str) -> Vec<String> {
        let mut out = Vec::new();
        for line in split_log_chunk(chunk) {
            out.extend(decide(t, &line).into_lines());
        }
        out
    }

    #[test]
    fn chunk_trailing_newline_does_not_add_blank_lines() {
        let mut t = ChildThrottle::default();
        let out = run(&mut t, "added 12 packages\n");
        assert_eq!(out, vec!["added 12 packages"]);
        let out = run(&mut t, "\n35 packages funding\n");
        assert_eq!(out, vec!["35 packages funding"]);
    }

    #[test]
    fn repeated_line_is_collapsed() {
        // 真实案例：某插件余额接口 401，两分钟刷了 60+ 条一模一样的行。
        let mut t = ChildThrottle::default();
        let spam = "[whale-balance] HTTP 余额接口请求失败: HTTP 401\n";
        let mut total = Vec::new();
        for _ in 0..60 {
            total.extend(run(&mut t, spam));
        }
        assert_eq!(total.len(), 1, "60 条重复只应写出 1 条, 实际: {total:?}");

        // 遇到不同的行时把折叠计数吐出来。
        let out = run(&mut t, "重启 DSH...\n");
        assert_eq!(out.len(), 2, "应先汇总再写新行, 实际: {out:?}");
        assert!(out[0].contains("重复 59 次"), "实际: {}", out[0]);
        assert_eq!(out[1], "重启 DSH...");
    }

    #[test]
    fn distinct_lines_are_rate_capped() {
        let mut t = ChildThrottle::default();
        let mut written = 0;
        for i in 0..250 {
            written += run(&mut t, &format!("line-{i}\n"))
                .iter()
                .filter(|l| l.starts_with("line-"))
                .count();
        }
        assert_eq!(
            written, CHILD_RATE_MAX as usize,
            "应被限到每分钟 {CHILD_RATE_MAX} 条"
        );
    }

    #[test]
    fn suppressed_line_is_not_referenced_by_repeat_summary() {
        let mut t = ChildThrottle::default();
        for i in 0..250 {
            run(&mut t, &format!("line-{i}\n"));
        }
        // 被限频丢掉的那行不该成为"上一行"。
        assert!(t.last_line.is_none());
    }
}
