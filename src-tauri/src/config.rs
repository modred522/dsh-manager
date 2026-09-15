//! 配置读写。
//!
//! **刻意沿用 Electron 版的目录与文件格式**（`%APPDATA%\DshManager\config.json`），
//! 这样老用户的设置、7 天日志、`analyses/` 分析历史全部平滑继承，不需要迁移步骤。
//! 注意这不是 Tauri 的 `app_data_dir()`（那会给出 `%APPDATA%\com.modred522.dsh-manager`）。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct WindowBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Default for WindowBounds {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            width: 800,
            height: 700,
        }
    }
}

// camelCase 是硬要求：现有 config.json 的键就是 dshUrl / autoCheckOnStartup 这种写法，
// 用 snake_case 会读不到老用户的设置（相当于每次升级都把配置重置）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub dsh_url: String,
    pub auto_check_on_startup: bool,
    pub auto_check_interval_hours: u64,
    pub auto_start_with_windows: bool,
    pub create_desktop_shortcut: bool,
    pub minimize_to_tray_on_startup: bool,
    // 费用估算单价（元 / 每百万 token），可在用量页调整
    pub cost_input: f64,
    pub cost_cache: f64,
    pub cost_output: f64,
    // 行为
    /// 更新通道：`all` = 取所有 dist-tags 的最高版本（能发现挂在 next 上的 rc/alpha）；
    /// `latest` = 只认 npm 的 latest 标签。默认沿用 `all`，但预发布版会明确标注。
    pub update_channel: String,
    pub watchdog: bool,
    pub clean_analysis_sessions: bool,
    pub auto_update_manager: bool,
    /// 界面语言：`system` | `zh` | `en`
    pub language: String,
    /// 主题：`system` | `light` | `dark`
    pub theme: String,
    pub window_bounds: Option<WindowBounds>,
    /// 可回滚到的上一个 dsh 版本
    pub rollback_version: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dsh_url: "http://127.0.0.1:3080".into(),
            auto_check_on_startup: true,
            auto_check_interval_hours: 6,
            auto_start_with_windows: false,
            create_desktop_shortcut: true,
            minimize_to_tray_on_startup: false,
            cost_input: 2.0,
            cost_cache: 0.5,
            cost_output: 8.0,
            update_channel: "all".into(),
            watchdog: true,
            clean_analysis_sessions: true,
            auto_update_manager: true,
            language: "system".into(),
            theme: "system".into(),
            window_bounds: None,
            rollback_version: None,
        }
    }
}

/// 配置目录覆盖（受限环境 / 烟测用）。
///
/// 存在的意义：不能靠重定向 `APPDATA` 来隔离测试环境 —— npm 的全局前缀默认就是
/// `%APPDATA%\npm`，一改 `APPDATA`，`npm prefix -g` 就跟着变，管理器会找不到已装的
/// dsh 并报「无法获取版本信息」。所以隔离必须只针对管理器自己的数据目录。
/// （Electron 版的 `DSH_USER_DATA` 是同一类逃生口。）
pub const CONFIG_DIR_ENV: &str = "DSH_MANAGER_DATA";

/// `%APPDATA%\DshManager`（与 Electron 版同一个目录）。
pub fn config_dir() -> PathBuf {
    resolve_config_dir(
        std::env::var_os(CONFIG_DIR_ENV).as_deref(),
        std::env::var_os("APPDATA").as_deref(),
    )
}

/// 目录解析逻辑单独抽出来：这样单测不必去改进程级环境变量
/// （Rust 测试默认并行跑，改全局环境变量会互相干扰）。
fn resolve_config_dir(
    override_dir: Option<&std::ffi::OsStr>,
    appdata: Option<&std::ffi::OsStr>,
) -> PathBuf {
    if let Some(dir) = override_dir {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    PathBuf::from(appdata.unwrap_or_default()).join("DshManager")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn log_dir() -> PathBuf {
    config_dir().join("logs")
}

pub fn analyses_dir() -> PathBuf {
    config_dir().join("analyses")
}

pub fn analysis_sessions_dir() -> PathBuf {
    config_dir().join("analysis-sessions")
}

/// 读配置。文件缺失或解析失败都退回默认值（与 Electron 版一致，不让坏配置卡住启动）。
pub fn load() -> Config {
    match std::fs::read_to_string(config_path()) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| Config::default()),
        Err(_) => Config::default(),
    }
}

pub fn save(cfg: &Config) -> std::io::Result<()> {
    std::fs::create_dir_all(config_dir())?;
    let text = serde_json::to_string_pretty(cfg)?;
    std::fs::write(config_path(), text)
}

/// 从 `dsh_url` 解析端口，解析不出来退回 3080。
pub fn dsh_port(cfg: &Config) -> u16 {
    cfg.dsh_url
        .rsplit(':')
        .next()
        .and_then(|tail| {
            let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse().ok()
        })
        .filter(|p| *p > 0)
        .unwrap_or(3080)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_electron_version() {
        let c = Config::default();
        assert_eq!(c.dsh_url, "http://127.0.0.1:3080");
        assert_eq!(c.auto_check_interval_hours, 6);
        assert_eq!(c.update_channel, "all");
        assert!(c.watchdog);
        assert!(!c.auto_start_with_windows);
    }

    #[test]
    fn missing_keys_fall_back_to_defaults() {
        // 老版本写的 config.json 不会有 updateChannel，必须能读且用默认值。
        let cfg: Config = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert_eq!(cfg.theme, "dark");
        assert_eq!(cfg.update_channel, "all");
        assert_eq!(cfg.dsh_url, "http://127.0.0.1:3080");
    }

    #[test]
    fn reads_real_electron_config_verbatim() {
        // 这是本机 %APPDATA%\DshManager\config.json 的真实内容（camelCase）。
        // 读不出来就意味着升级到 Tauri 版会把用户设置全部重置。
        let real = r#"{
          "dshUrl": "http://127.0.0.1:3080",
          "autoCheckOnStartup": true,
          "autoCheckIntervalHours": 6,
          "autoStartWithWindows": false,
          "createDesktopShortcut": true,
          "minimizeToTrayOnStartup": false,
          "costInput": 2,
          "costCache": 0.5,
          "costOutput": 8,
          "watchdog": true,
          "cleanAnalysisSessions": true,
          "language": "system",
          "theme": "system",
          "windowBounds": { "x": 1912, "y": -8, "width": 1936, "height": 1048 },
          "rollbackVersion": "0.1.5-rc.2"
        }"#;
        let cfg: Config = serde_json::from_str(real).expect("真实配置必须能解析");
        assert_eq!(cfg.dsh_url, "http://127.0.0.1:3080");
        assert_eq!(cfg.auto_check_interval_hours, 6);
        assert_eq!(cfg.cost_cache, 0.5);
        assert_eq!(cfg.rollback_version.as_deref(), Some("0.1.5-rc.2"));
        let b = cfg.window_bounds.expect("windowBounds 应解析出来");
        assert_eq!((b.x, b.y, b.width, b.height), (1912, -8, 1936, 1048));
        // 老配置没有 updateChannel / autoUpdateManager，走默认值。
        assert_eq!(cfg.update_channel, "all");
        assert!(cfg.auto_update_manager);
    }

    #[test]
    fn round_trip_keeps_camel_case_keys() {
        let text = serde_json::to_string(&Config::default()).unwrap();
        assert!(
            text.contains("\"dshUrl\""),
            "序列化必须仍是 camelCase: {text}"
        );
        assert!(text.contains("\"autoCheckIntervalHours\""));
        assert!(!text.contains("\"dsh_url\""));
    }

    #[test]
    fn broken_json_does_not_panic() {
        let cfg: Config = serde_json::from_str("{not json").unwrap_or_default();
        assert_eq!(cfg.update_channel, "all");
    }

    #[test]
    fn config_dir_prefers_override_then_appdata() {
        use std::ffi::OsStr;
        let probe = OsStr::new("D:/tmp/probe");
        let appdata = OsStr::new("C:/Users/x/AppData/Roaming");

        // 覆盖变量优先。
        assert_eq!(
            resolve_config_dir(Some(probe), Some(appdata)),
            PathBuf::from(probe)
        );
        // 没有覆盖时落到 %APPDATA%\DshManager。
        assert_eq!(
            resolve_config_dir(None, Some(appdata)),
            PathBuf::from(appdata).join("DshManager")
        );
        // 空字符串视为没设置（避免 set 了个空值就把数据写到盘根）。
        assert_eq!(
            resolve_config_dir(Some(OsStr::new("")), Some(appdata)),
            PathBuf::from(appdata).join("DshManager")
        );
    }

    #[test]
    fn port_parsing() {
        let mut c = Config::default();
        assert_eq!(dsh_port(&c), 3080);
        c.dsh_url = "http://127.0.0.1:9999".into();
        assert_eq!(dsh_port(&c), 9999);
        c.dsh_url = "http://127.0.0.1:8080/path".into();
        assert_eq!(dsh_port(&c), 8080);
        // 没有端口时退回默认。
        c.dsh_url = "http://localhost".into();
        assert_eq!(dsh_port(&c), 3080);
    }
}
