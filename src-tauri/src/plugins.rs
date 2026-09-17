//! 已安装插件的管理：列出、装、卸、升级、查可升级版本。
//!
//! 已安装插件 = `$DSH_HOME/profiles/web/package.json` 的 `dependencies`。
//! 增删走 `dsh plugin --profile web add|remove <pkg>`（dsh 内部转发 pnpm），
//! **不要手改 package.json** —— `dsh plugin add` 还会同步写 `dsh.profile.bundles`
//! （见 docs/GOTCHAS.md 四.2）。

use serde::Serialize;

use crate::dsh;
use crate::logging;
use crate::usage::dsh_home;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Plugin {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginUpdate {
    pub name: String,
    pub version: String,
    pub latest: Option<String>,
    pub has_update: bool,
    /// git/github/本地路径来源的插件查不到 npm 版本，界面上不显示升级按钮。
    pub updatable: bool,
}

/// 列出 web profile 里已装的插件。
pub fn profile_plugins() -> Vec<Plugin> {
    let path = dsh_home().join("profiles").join("web").join("package.json");
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let Some(deps) = pkg.get("dependencies").and_then(|d| d.as_object()) else {
        return Vec::new();
    };

    let mut out: Vec<Plugin> = deps
        .iter()
        .map(|(name, version)| Plugin {
            name: name.clone(),
            version: strip_range(version.as_str().unwrap_or("")),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 去掉 `^` / `~` 前缀，只留版本号本身。
fn strip_range(v: &str) -> String {
    v.trim_start_matches(['^', '~']).to_string()
}

/// 非 npm 来源（github:/git+/http(s):/file:/相对路径）查不到 dist-tags。
pub fn is_non_npm_source(name: &str) -> bool {
    name.starts_with("github:")
        || name.starts_with("git+")
        || name.starts_with("http:")
        || name.starts_with("https:")
        || name.starts_with("file:")
        || name.starts_with("./")
        || name.starts_with("../")
        || name.starts_with(".\\")
        || name.starts_with("..\\")
}

/// 跑一条 `dsh plugin --profile web <args...>`，输出逐行进日志。返回退出码（起不来为 -1）。
pub async fn run_plugin_command(args: &[&str]) -> i32 {
    let line = format!("dsh plugin --profile web {}", args.join(" "));
    let mut cmd = dsh::shell_command(&line);
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null());

    let Ok(mut child) = cmd.spawn() else {
        return -1;
    };

    use tokio::io::{AsyncBufReadExt, BufReader};
    let mut tasks = Vec::new();
    if let Some(out) = child.stdout.take() {
        tasks.push(tokio::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                logging::log_child_output(&l);
            }
        }));
    }
    if let Some(err) = child.stderr.take() {
        tasks.push(tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                logging::log_child_output(&l);
            }
        }));
    }

    let code = child.wait().await.ok().and_then(|s| s.code()).unwrap_or(-1);
    for t in tasks {
        let _ = t.await;
    }
    code
}

/// 逐个查已装插件的最新版本。非 npm 来源跳过（标 `updatable: false`）。
pub async fn check_updates(channel: &str) -> Vec<PluginUpdate> {
    let mut out = Vec::new();
    for p in profile_plugins() {
        if is_non_npm_source(&p.name) {
            out.push(PluginUpdate {
                name: p.name,
                version: p.version,
                latest: None,
                has_update: false,
                updatable: false,
            });
            continue;
        }
        let latest = crate::updates::latest_version_for(&p.name, channel).await;
        let has_update = latest
            .as_deref()
            .map(|l| crate::pure::compare_versions(l, &p.version) == std::cmp::Ordering::Greater)
            .unwrap_or(false);
        out.push(PluginUpdate {
            name: p.name,
            version: p.version,
            latest,
            has_update,
            updatable: true,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_semver_range_prefix() {
        assert_eq!(strip_range("^1.2.3"), "1.2.3");
        assert_eq!(strip_range("~0.1.0"), "0.1.0");
        assert_eq!(strip_range("1.0.0"), "1.0.0");
        // git 来源的"版本"其实是个地址，原样保留。
        assert_eq!(
            strip_range("github:MeteorNOX/DeepSeek-Balance-Whale-Widget"),
            "github:MeteorNOX/DeepSeek-Balance-Whale-Widget"
        );
    }

    #[test]
    fn detects_non_npm_sources() {
        assert!(is_non_npm_source("github:owner/repo"));
        assert!(is_non_npm_source("git+https://x/y.git"));
        assert!(is_non_npm_source("file:../local"));
        assert!(is_non_npm_source("./rel"));
        assert!(!is_non_npm_source("@liustack/modlens"));
        assert!(!is_non_npm_source("dsh-whale-widget"));
    }

    #[test]
    fn reads_this_machine_profile_if_present() {
        // 本机装了 dsh，profile 存在时应能列出插件且不 panic。
        let list = profile_plugins();
        for p in &list {
            assert!(!p.name.is_empty());
        }
        // 列表必须按名字有序（否则界面每次刷新顺序乱跳）。
        let mut sorted = list.clone();
        sorted.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(list, sorted);
    }

    #[test]
    fn parses_dependencies_shape() {
        let pkg: serde_json::Value = serde_json::from_str(
            r#"{"dependencies":{"b-pkg":"^2.0.0","a-pkg":"~1.1.0",
                "dsh-whale-widget":"github:MeteorNOX/DeepSeek-Balance-Whale-Widget"}}"#,
        )
        .unwrap();
        let deps = pkg["dependencies"].as_object().unwrap();
        let mut list: Vec<Plugin> = deps
            .iter()
            .map(|(n, v)| Plugin {
                name: n.clone(),
                version: strip_range(v.as_str().unwrap_or("")),
            })
            .collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(list[0].name, "a-pkg");
        assert_eq!(list[0].version, "1.1.0");
        assert_eq!(list[1].version, "2.0.0");
        assert!(is_non_npm_source(&list[2].version));
    }
}

#[cfg(test)]
mod wire_format {
    use super::*;

    /// renderer.js 的插件页按这些名字取字段，改名会让升级按钮静默失效。
    #[test]
    fn serialized_keys_match_renderer_expectations() {
        let v = serde_json::to_value(Plugin {
            name: "x".into(),
            version: "1.0.0".into(),
        })
        .unwrap();
        assert!(v.get("name").is_some() && v.get("version").is_some());

        let v = serde_json::to_value(PluginUpdate {
            name: "x".into(),
            version: "1.0.0".into(),
            latest: Some("1.1.0".into()),
            has_update: true,
            updatable: true,
        })
        .unwrap();
        for k in ["name", "version", "latest", "hasUpdate", "updatable"] {
            assert!(v.get(k).is_some(), "缺少 {k}: {v}");
        }
        assert!(v.get("has_update").is_none(), "不该有 snake_case 残留");
    }
}
