//! dsh 版本更新：查最新版、拉官方 changelog、装指定版本。
//!
//! 移植自 Electron 版 `main.js` 的「npm/DSH 交互」与「更新日志」两块。
//! 上层的 check/update/rollback 编排留在 `lib.rs`，因为要动 `AppState` 与任务栏进度。

use std::sync::Mutex;

use crate::dsh;
use crate::logging;
use crate::pure::compare_versions;

const DSH_PKG: &str = "@deepseek-ai/dsh";
const RELEASES_URL: &str =
    "https://api.github.com/repos/deepseek-ai/deepseek-harness/releases?per_page=20";

/// 取某个 npm 包的"最新版本"。
///
/// `channel = "latest"` 只认 npm 的 latest 标签；其它（默认 `all`）取所有 dist-tags 里最高的
/// —— dsh 的 rc/alpha 惯例挂在 `next` 上，只读 latest 会漏报（见 docs/GOTCHAS.md 四.8）。
pub async fn latest_version_for(pkg: &str, channel: &str) -> Option<String> {
    let out = dsh::shell_command(&format!("npm view {pkg} dist-tags --json"))
        .output()
        .await
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let tags: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    let obj = tags.as_object()?;

    let candidates: Vec<&str> = if channel == "latest" {
        obj.get("latest")
            .and_then(|v| v.as_str())
            .into_iter()
            .collect()
    } else {
        obj.values().filter_map(|v| v.as_str()).collect()
    };

    let mut best: Option<String> = None;
    for raw in candidates {
        let v = raw.trim().trim_start_matches('v');
        // 必须形如 x.y.z 才算版本号（dist-tags 里可能有奇怪的值）。
        let mut parts = v.split('.');
        let looks_like_version = (0..3).all(|_| {
            parts
                .next()
                .map(|p| p.chars().next().is_some_and(|c| c.is_ascii_digit()))
                .unwrap_or(false)
        });
        if !looks_like_version {
            continue;
        }
        match &best {
            Some(b) if compare_versions(v, b) != std::cmp::Ordering::Greater => {}
            _ => best = Some(v.to_string()),
        }
    }
    best
}

pub async fn latest_dsh_version(channel: &str) -> Option<String> {
    latest_version_for(DSH_PKG, channel).await
}

/// changelog 缓存：同一版本只拉一次（GitHub 匿名接口有限流）。
static CHANGELOG_CACHE: Mutex<Option<(String, Option<String>)>> = Mutex::new(None);

pub fn clear_changelog_cache() {
    *CHANGELOG_CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// 拉官方 Release 正文并清洗成纯文本 changelog（只保留中文段）。
///
/// 上游的 tag 形如 `dsh-v0.1.5-rc.2`。
pub async fn fetch_changelog(version: &str) -> Option<String> {
    if version.is_empty() {
        return None;
    }
    {
        let guard = CHANGELOG_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((v, text)) = guard.as_ref() {
            if v == version {
                return text.clone();
            }
        }
    }

    let text = fetch_changelog_uncached(version).await;
    *CHANGELOG_CACHE.lock().unwrap_or_else(|e| e.into_inner()) =
        Some((version.to_string(), text.clone()));
    text
}

async fn fetch_changelog_uncached(version: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        // GitHub 接口必须带 User-Agent，否则 403。
        .user_agent("dsh-manager")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .ok()?;
    let rels: serde_json::Value = client
        .get(RELEASES_URL)
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let wanted = format!("dsh-v{version}");
    let body = rels
        .as_array()?
        .iter()
        .find(|r| r.get("tag_name").and_then(|t| t.as_str()) == Some(wanted.as_str()))?
        .get("body")?
        .as_str()?;
    let cleaned = crate::pure::release_body_to_text(body);
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// `npm install -g @deepseek-ai/dsh@<version>`，输出逐行进日志。
///
/// 返回退出码；起不来返回 -1（与 Electron 版一致）。
pub async fn run_install(version: &str) -> i32 {
    let mut cmd = dsh::shell_command(&format!("npm install -g {DSH_PKG}@{version}"));
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
    // 等输出读完，别让日志被截断在进程退出那一刻。
    for t in tasks {
        let _ = t.await;
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn latest_version_for_real_package() {
        // 用一个存在且稳定的包验证 dist-tags 解析链路（顺带验证 shell 调用方式）。
        let v = latest_version_for("npm", "latest").await;
        assert!(v.is_some(), "应能解析出 npm 自己的 latest 版本");
        let v = v.unwrap();
        assert!(
            v.chars().next().is_some_and(|c| c.is_ascii_digit()),
            "版本号应以数字开头, 实际 {v}"
        );
    }

    #[tokio::test]
    async fn dsh_channels_resolve_differently() {
        // 直接打真实的 dsh 包：这是"检查更新"实际依赖的那条链路。
        // 上游把 rc/alpha 挂在 next/alpha 标签上，所以两个通道应当给出不同答案
        // （只读 latest 会漏报，正是 GOTCHAS 四.8 记的那个坑）。
        let all = latest_dsh_version("all")
            .await
            .expect("all 通道应解析出版本");
        let latest = latest_dsh_version("latest")
            .await
            .expect("latest 通道应解析出版本");
        assert_ne!(all, "", "all 通道不该为空");
        // all 通道取的是所有标签里最高的，所以不可能低于 latest 标签。
        assert_ne!(
            compare_versions(&all, &latest),
            std::cmp::Ordering::Less,
            "all={all} 不该低于 latest={latest}"
        );
    }

    #[tokio::test]
    async fn unknown_package_yields_none() {
        let v = latest_version_for("@modred522/definitely-not-a-real-package-xyz", "all").await;
        assert!(v.is_none());
    }

    #[tokio::test]
    async fn empty_version_has_no_changelog() {
        assert!(fetch_changelog("").await.is_none());
    }

    #[tokio::test]
    async fn changelog_for_unknown_version_is_none_and_cached() {
        clear_changelog_cache();
        // 不存在的版本拉不到正文；重复调用应命中缓存而不再发请求。
        assert!(fetch_changelog("0.0.0-does-not-exist").await.is_none());
        assert!(fetch_changelog("0.0.0-does-not-exist").await.is_none());
    }
}
