//! 主进程界面文案（托盘 / 通知 / 窗口标题）。
//!
//! 对应 Electron 版 `main.js` 的 `MAIN_TEXTS`。渲染层的 175 个 key 仍住在
//! `renderer/i18n.js`（原样复用），这里只管 Rust 侧要显示的那几条。
//! **日志正文保持中文**，属技术输出，不走 i18n —— 与 Electron 版同一约定。

use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    AppTitle,
    MarketTitle,
    TrayOpen,
    TrayOpenDsh,
    TrayRestart,
    TrayStop,
    TrayCheck,
    TrayShortcut,
    TrayQuit,
    TrayTooltip,
    NotifyUpdateTitle,
}

/// 解析界面语言：`system` 时跟随系统（非中文环境用英文）。
///
/// 系统语言只探测一次并缓存：`t()` 会被托盘菜单、窗口标题反复调用，
/// 每次都去查区域设置纯属浪费（早先用 PowerShell 查更是每次起一个进程）。
fn resolve(lang: &str) -> &'static str {
    match lang {
        "zh" => "zh",
        "en" => "en",
        _ => {
            static SYSTEM_LANG: OnceLock<&'static str> = OnceLock::new();
            SYSTEM_LANG.get_or_init(|| {
                match sys_locale::get_locale() {
                    // 形如 zh-CN / zh-Hans-CN。
                    Some(l) if l.to_ascii_lowercase().starts_with("zh") => "zh",
                    Some(_) => "en",
                    // 取不到就按中文（作者与主要用户群是中文环境，与 Electron 版一致）。
                    None => "zh",
                }
            })
        }
    }
}

pub fn t(lang: &str, key: Key) -> String {
    let zh = resolve(lang) == "zh";
    match (key, zh) {
        (Key::AppTitle, true) => "DSH 管理器",
        (Key::AppTitle, false) => "DSH Manager",
        (Key::MarketTitle, true) => "DSH 插件市场",
        (Key::MarketTitle, false) => "DSH Plugin Marketplace",
        (Key::TrayOpen, true) => "打开管理器",
        (Key::TrayOpen, false) => "Open Manager",
        (Key::TrayOpenDsh, true) => "打开 DSH",
        (Key::TrayOpenDsh, false) => "Open DSH",
        (Key::TrayRestart, true) => "重启 DSH",
        (Key::TrayRestart, false) => "Restart DSH",
        (Key::TrayStop, true) => "停止 DSH",
        (Key::TrayStop, false) => "Stop DSH",
        (Key::TrayCheck, true) => "检查更新",
        (Key::TrayCheck, false) => "Check for Updates",
        (Key::TrayShortcut, true) => "创建桌面快捷方式",
        (Key::TrayShortcut, false) => "Create Desktop Shortcut",
        (Key::TrayQuit, true) => "退出",
        (Key::TrayQuit, false) => "Quit",
        (Key::TrayTooltip, true) => "DSH 管理器（Ctrl+Alt+D 打开 DSH）",
        (Key::TrayTooltip, false) => "DSH Manager (Ctrl+Alt+D opens DSH)",
        (Key::NotifyUpdateTitle, true) => "DSH 有更新",
        (Key::NotifyUpdateTitle, false) => "DSH Update Available",
    }
    .to_string()
}

pub fn tray_tooltip_running(lang: &str, n: usize) -> String {
    if resolve(lang) == "zh" {
        format!("DSH 管理器 — DSH 运行中（{n} 个进程）")
    } else {
        format!("DSH Manager — DSH running ({n} processes)")
    }
}

/// 托盘"停止 DSH"带进程数的形式。
pub fn tray_stop_label(lang: &str, n: usize) -> String {
    if n == 0 {
        return t(lang, Key::TrayStop);
    }
    if resolve(lang) == "zh" {
        format!("停止 DSH（{n} 个进程）")
    } else {
        format!("Stop DSH ({n} processes)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_language_wins_over_system() {
        assert_eq!(t("zh", Key::TrayQuit), "退出");
        assert_eq!(t("en", Key::TrayQuit), "Quit");
        assert_eq!(t("zh", Key::AppTitle), "DSH 管理器");
        assert_eq!(t("en", Key::AppTitle), "DSH Manager");
    }

    #[test]
    fn plural_labels_interpolate_count() {
        assert!(tray_tooltip_running("zh", 2).contains('2'));
        assert!(tray_stop_label("en", 3).contains('3'));
        // 0 个进程时退回不带数字的说法。
        assert_eq!(tray_stop_label("zh", 0), "停止 DSH");
    }

    #[test]
    fn every_key_has_both_languages() {
        for key in [
            Key::AppTitle,
            Key::MarketTitle,
            Key::TrayOpen,
            Key::TrayOpenDsh,
            Key::TrayRestart,
            Key::TrayStop,
            Key::TrayCheck,
            Key::TrayShortcut,
            Key::TrayQuit,
            Key::TrayTooltip,
            Key::NotifyUpdateTitle,
        ] {
            assert!(!t("zh", key).is_empty(), "缺中文: {key:?}");
            assert!(!t("en", key).is_empty(), "缺英文: {key:?}");
            assert_ne!(
                t("zh", key),
                t("en", key),
                "中英文案相同，可能漏译: {key:?}"
            );
        }
    }
}
