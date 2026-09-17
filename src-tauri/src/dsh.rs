//! 与 npm / dsh CLI 的交互：解析 npm 前缀、读已装版本、探活、启停。

use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::logging;

/// Windows 下隐藏子进程控制台窗口（对应 Electron 的 `windowsHide: true`）。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 我们自己拉起的那个 dsh 子进程。
///
/// 只存 pid 和存活标记，不把 `Child` 塞进 Mutex —— `Child` 需要被 await，
/// 持锁跨 await 是死锁温床。真正的 `Child` 由 spawn 出去的任务独占。
#[derive(Debug, Default, Clone, Copy)]
pub struct ChildInfo {
    pub pid: Option<u32>,
    pub alive: bool,
    /// 曾经拉起过又退出了（watchdog 判断"异常退出"的依据）。
    pub exited_unexpectedly: bool,
}

static CHILD: Mutex<ChildInfo> = Mutex::new(ChildInfo {
    pid: None,
    alive: false,
    exited_unexpectedly: false,
});

pub fn child_info() -> ChildInfo {
    *CHILD.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn is_dsh_running() -> bool {
    child_info().alive
}

/// 清掉"异常退出"标记（watchdog 处理完或用户手动重启后调用）。
pub fn clear_child(mark_handled: bool) {
    let mut g = CHILD.lock().unwrap_or_else(|e| e.into_inner());
    g.pid = None;
    g.alive = false;
    if mark_handled {
        g.exited_unexpectedly = false;
    }
}

/// 构造一条"经 shell 执行"的命令。
///
/// **Windows 上必须这么做**：`npm` / `dsh` 实际是 `npm.cmd` / `dsh.cmd`（npm shim），
/// 而 Rust 的 `Command::new` 不像 cmd.exe 那样按 PATHEXT 补后缀，直接
/// `Command::new("npm")` 会报找不到程序。Electron 版用 Node 的 `exec()` 过 shell，
/// 所以从没撞上这条 —— 移植时是单测替我抓出来的。
pub fn shell_command(line: &str) -> Command {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("cmd.exe");
        cmd.args(["/c", line]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    }
    #[cfg(not(windows))]
    {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", line]);
        cmd
    }
}

/// `npm prefix -g`，用来定位全局安装的 dsh。
pub async fn resolve_npm_prefix() -> String {
    match shell_command("npm prefix -g").output().await {
        Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        Err(_) => String::new(),
    }
}

fn dsh_package_json(npm_prefix: &str) -> std::path::PathBuf {
    std::path::Path::new(npm_prefix)
        .join("node_modules")
        .join("@deepseek-ai")
        .join("dsh")
        .join("package.json")
}

/// 已安装的 dsh 版本（读全局 node_modules 里的 package.json，比 `dsh --version` 快且不起进程）。
pub fn installed_version(npm_prefix: &str) -> Option<String> {
    let text = std::fs::read_to_string(dsh_package_json(npm_prefix)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.get("version")?.as_str().map(|s| s.to_string())
}

/// dsh 本体的入口脚本路径（分析管线要直接 spawn 它）。
pub fn dsh_bin(npm_prefix: &str) -> std::path::PathBuf {
    std::path::Path::new(npm_prefix)
        .join("node_modules")
        .join("@deepseek-ai")
        .join("dsh")
        .join("lib")
        .join("bin.js")
}

/// HTTP 探活。失败/超时都算"没起来"，不区分原因。
pub async fn is_server_up(url: &str, timeout: Duration) -> bool {
    let Ok(client) = reqwest::Client::builder().timeout(timeout).build() else {
        return false;
    };
    client.get(url).send().await.is_ok()
}

/// 启动 `dsh web`，把 stdout/stderr 逐行转发进日志。
///
/// 走 `cmd.exe /c` 是为了用上 npm 的 .cmd shim（和 Electron 版一致）。
pub fn launch_dsh() -> std::io::Result<()> {
    let mut cmd = shell_command("dsh web");
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    let mut child = cmd.spawn()?;
    let pid = child.id();

    {
        let mut g = CHILD.lock().unwrap_or_else(|e| e.into_inner());
        g.pid = pid;
        g.alive = true;
        g.exited_unexpectedly = false;
    }

    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                logging::log_child_output(&line);
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                logging::log_child_output(&line);
            }
        });
    }

    // Child 交给这个任务独占，退出时更新标记供 watchdog 读取。
    tokio::spawn(async move {
        let _ = child.wait().await;
        let mut g = CHILD.lock().unwrap_or_else(|e| e.into_inner());
        g.alive = false;
        g.exited_unexpectedly = true;
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_read_from_missing_prefix_is_none() {
        assert!(installed_version("Z:\\definitely-not-here").is_none());
    }

    #[test]
    fn paths_are_built_as_expected() {
        let p = dsh_bin("C:\\npm");
        let s = p.to_string_lossy().replace('\\', "/");
        assert!(
            s.ends_with("npm/node_modules/@deepseek-ai/dsh/lib/bin.js"),
            "实际 {s}"
        );
    }

    #[test]
    fn child_state_starts_clean() {
        // 没启动过时不该被当成运行中。
        let info = child_info();
        assert!(!info.alive || info.pid.is_some());
    }

    #[tokio::test]
    async fn shell_command_can_run_a_cmd_shim() {
        // 直接 Command::new("npm") 在 Windows 上会找不到 npm.cmd，
        // 所以所有 npm/dsh 调用都必须走 shell_command()。
        let out = shell_command("npm --version")
            .output()
            .await
            .expect("shell 应能执行");
        let ver = String::from_utf8_lossy(&out.stdout);
        assert!(
            ver.trim().starts_with(char::is_numeric),
            "应拿到 npm 版本号, 实际 {ver:?}"
        );
    }

    #[tokio::test]
    async fn npm_prefix_resolves_on_this_machine() {
        // 本机装了 npm；解析不出来说明命令调用方式有问题。
        let prefix = resolve_npm_prefix().await;
        assert!(!prefix.is_empty(), "npm prefix -g 应返回路径");
    }

    #[tokio::test]
    async fn server_probe_on_dead_port_is_false() {
        assert!(!is_server_up("http://127.0.0.1:65001", Duration::from_millis(300)).await);
    }
}
