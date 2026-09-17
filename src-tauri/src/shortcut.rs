//! 桌面快捷方式。
//!
//! 这是整个迁移里唯一**没有** Tauri 等价能力的部分：Electron 有
//! `shell.writeShortcutLink`，Tauri 没有。所以直接走 COM 的 `IShellLink` +
//! `IPersistFile`，代价是一段 unsafe，但一次写完就不用再碰；
//! 备选方案（保留一个 PowerShell 辅助脚本）会把 GOTCHAS 第一节那堆
//! PS 5.1 编码坑重新请回来，不值得。

use std::path::{Path, PathBuf};

/// 快捷方式文件名，与 Electron 版保持一致（否则用户桌面上会出现两个）。
pub const LINK_NAME: &str = "DSH 管理器.lnk";

pub struct ShortcutSpec<'a> {
    pub link_path: &'a Path,
    pub target: &'a Path,
    pub args: &'a str,
    pub working_dir: &'a Path,
    pub icon: &'a Path,
    pub description: &'a str,
}

#[cfg(windows)]
pub fn write_shortcut(spec: &ShortcutSpec<'_>) -> Result<(), String> {
    use windows::core::{Interface, HSTRING};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

    unsafe {
        // 已初始化过会返回 S_FALSE，不是错误，忽略即可。
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let result = (|| -> Result<(), String> {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("创建 ShellLink 失败: {e}"))?;

            link.SetPath(&HSTRING::from(spec.target.as_os_str()))
                .map_err(|e| format!("SetPath 失败: {e}"))?;
            if !spec.args.is_empty() {
                link.SetArguments(&HSTRING::from(spec.args))
                    .map_err(|e| format!("SetArguments 失败: {e}"))?;
            }
            link.SetWorkingDirectory(&HSTRING::from(spec.working_dir.as_os_str()))
                .map_err(|e| format!("SetWorkingDirectory 失败: {e}"))?;
            // Windows 任务栏/桌面认 ICO；PNG 只在标题栏生效（见 GOTCHAS 交付清单最后一条）。
            link.SetIconLocation(&HSTRING::from(spec.icon.as_os_str()), 0)
                .map_err(|e| format!("SetIconLocation 失败: {e}"))?;
            link.SetDescription(&HSTRING::from(spec.description))
                .map_err(|e| format!("SetDescription 失败: {e}"))?;

            let persist: IPersistFile = link
                .cast()
                .map_err(|e| format!("取 IPersistFile 失败: {e}"))?;
            persist
                .Save(&HSTRING::from(spec.link_path.as_os_str()), true)
                .map_err(|e| format!("写入 .lnk 失败: {e}"))?;
            Ok(())
        })();

        CoUninitialize();
        result
    }
}

#[cfg(not(windows))]
pub fn write_shortcut(_spec: &ShortcutSpec<'_>) -> Result<(), String> {
    Err("桌面快捷方式仅在 Windows 上支持".into())
}

/// 在桌面创建「DSH 管理器」快捷方式。
///
/// `only_if_missing` 为真时，已存在就直接返回 `Ok(None)`（启动时的自动补建走这个分支，
/// 避免每次启动都重写文件）。返回 `Ok(Some(path))` 表示这次真的写了。
pub fn create_desktop_shortcut(
    desktop_dir: &Path,
    exe: &Path,
    icon: &Path,
    only_if_missing: bool,
) -> Result<Option<PathBuf>, String> {
    let link_path = desktop_dir.join(LINK_NAME);
    if only_if_missing && link_path.exists() {
        return Ok(None);
    }
    let working_dir = exe.parent().unwrap_or(desktop_dir);
    write_shortcut(&ShortcutSpec {
        link_path: &link_path,
        target: exe,
        // Tauri 版的 exe 本身就是应用，不需要像开发模式的 Electron 那样传项目目录。
        args: "",
        working_dir,
        icon,
        description: "DSH 管理器",
    })?;
    Ok(Some(link_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_name_matches_electron_version() {
        // 名字变了用户桌面上就会出现两个快捷方式。
        assert_eq!(LINK_NAME, "DSH 管理器.lnk");
    }

    #[test]
    fn only_if_missing_skips_existing_file() {
        let dir = std::env::temp_dir().join("dsh-mgr-shortcut-test-skip");
        std::fs::create_dir_all(&dir).unwrap();
        let existing = dir.join(LINK_NAME);
        std::fs::write(&existing, b"placeholder").unwrap();

        let r = create_desktop_shortcut(
            &dir,
            Path::new("C:\\Windows\\System32\\notepad.exe"),
            Path::new("C:\\Windows\\System32\\notepad.exe"),
            true,
        );
        assert_eq!(r, Ok(None), "已存在时不该重写");
        // 确认原文件没被动过。
        assert_eq!(std::fs::read(&existing).unwrap(), b"placeholder");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn writes_a_real_lnk_file() {
        let dir = std::env::temp_dir().join("dsh-mgr-shortcut-test-write");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let exe = Path::new("C:\\Windows\\System32\\notepad.exe");

        let made = create_desktop_shortcut(&dir, exe, exe, false).expect("应能写出快捷方式");
        let path = made.expect("只要没跳过就应返回路径");
        assert!(path.exists(), "应真的落盘一个 .lnk");

        // .lnk 的魔数：头 4 字节是 0x4C 长度字段。
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.len() > 76, "文件太小，不像有效 .lnk");
        assert_eq!(
            &bytes[0..4],
            &[0x4C, 0x00, 0x00, 0x00],
            "缺少 .lnk 头部魔数"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
