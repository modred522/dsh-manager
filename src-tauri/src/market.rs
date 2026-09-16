//! 插件市场（npm / GitHub 双源）：搜索分页与详情抓取。只读、纯网络。
//!
//! 移植自 Electron 版 `lib/market.js`。
//!
//! 排序口径保持不变：GitHub 固定 `sort=stars`（星标降序）；
//! npm 走 registry 默认相关度排序（内部综合 质量/维护/流行度 打分，不是纯下载量）。

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;

use crate::pure::normalize_github_url;

const NPM_PAGE_SIZE: usize = 25; // npm 接口每页条数
const GH_PAGE_SIZE: usize = 20; // GitHub 接口每页条数
const MARKET_BATCH: usize = 20; // 每次返回给渲染层的条数
const NPM_MAX_FROM: usize = 500; // npm 搜索偏移上限（防御性终止）
const GH_MAX_PAGE: usize = 50; // GitHub 搜索最多 1000 条（20 条/页 × 50 页）

fn client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        // GitHub 接口必须带 User-Agent，否则 403。
        .user_agent("dsh-manager")
        .timeout(Duration::from_secs(20))
        .build()
        .ok()
}

/// 环境里配的代理（去掉可能带的用户名密码）。
///
/// reqwest 会自动认这些变量，而 curl / 浏览器未必走同一套 —— "命令行能通、
/// 应用里不通"十有八九是这里。所以连接失败时要把它报出来。
fn proxy_hint() -> Option<String> {
    for key in ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"] {
        let Ok(v) = std::env::var(key) else { continue };
        let v = v.trim();
        if v.is_empty() {
            continue;
        }
        return Some(format!("{key}={}", mask_credentials(v)));
    }
    None
}

/// `http://user:pass@host:port` -> `http://host:port`
fn mask_credentials(url: &str) -> String {
    match (url.find("://"), url.rfind('@')) {
        (Some(i), Some(at)) if at > i => format!("{}{}", &url[..i + 3], &url[at + 1..]),
        _ => url.to_string(),
    }
}

/// 把 reqwest 的错误翻成一句能直接指导排查的话。
///
/// 这些失败以前一律被压成 `None`，界面只好说"可能被限流或仓库不存在"——
/// 限流、404、代理没开、TLS 失败、超时全长一个样，用户没法自查，
/// 隔着一台机器也判断不出来。
fn describe_error(e: &reqwest::Error) -> String {
    let kind = if e.is_timeout() {
        "请求超时"
    } else if e.is_connect() {
        "连接失败"
    } else if e.is_decode() {
        "响应解析失败"
    } else {
        "请求失败"
    };
    // 根因（DNS 解析不了 / 连接被拒 / TLS 握手失败）比分类有用，逐层取到最里面那条。
    let mut cause: &dyn std::error::Error = e;
    while let Some(next) = std::error::Error::source(cause) {
        cause = next;
    }
    let mut out = format!("{kind}：{cause}");
    if e.is_connect() || e.is_timeout() {
        if let Some(p) = proxy_hint() {
            out.push_str(&format!("（当前走代理 {p}；代理没开就是这个现象）"));
        }
    }
    out
}

/// 接口返回的 `message` 会进日志、也会显示在界面上，转发前先处理两件事：
///
/// * **掩掉 IPv4** —— GitHub 的限流提示里带着本机的公网出口 IP。日志是会被
///   贴出来问人的，公司出口 IP 没必要跟着一起出去。
/// * **砍掉补充说明** —— 括号里那段对排查没用（见下面注释）。
fn sanitize_api_message(m: &str) -> String {
    const LIMIT: usize = 120;
    let mut out = mask_ipv4(m.trim());
    // 上游习惯把补充说明塞在括号里（GitHub 就爱挂一段"登录后额度更高"的推销）。
    // 正文在前面的话直接砍掉括号那段，比按长度硬切干净 —— 硬切会留下
    // "Check ou" 这种半截话。
    if let Some(i) = out.find(" (") {
        if i >= 10 {
            out.truncate(i); // i 指向空格，天然是字符边界
        }
    }
    let out = out.trim();
    if out.chars().count() > LIMIT {
        return format!("{}…", truncate_chars(out, LIMIT));
    }
    out.to_string()
}

fn mask_ipv4(s: &str) -> String {
    s.split(' ')
        .map(|tok| {
            // 句尾标点要留着，别把 "for 1.2.3.4." 的句号也吃掉。
            let core = tok.trim_end_matches(['.', ',', ')', ';', ':']);
            let looks_like_ip = core.split('.').count() == 4
                && core
                    .split('.')
                    .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
            if looks_like_ip {
                // core 全是数字和点，纯 ASCII，按字节切片是安全的。
                format!("<出口 IP>{}", &tok[core.len()..])
            } else {
                tok.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// GitHub 把限流解除时间写在 `x-ratelimit-reset`（unix 秒）里。
///
/// 未登录接口是**按出口 IP** 限 60 次/小时：公司网络走 NAT 的话，这 60 次是
/// 整个办公室共用的，随时可能不是自己用掉的。所以"什么时候能再试"比
/// "被限流了"有用得多。
fn rate_limit_reset_hint(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let remaining: u64 = headers
        .get("x-ratelimit-remaining")?
        .to_str()
        .ok()?
        .parse()
        .ok()?;
    // 还有余量说明 403 是别的原因（比如缺 User-Agent），别乱报限流。
    if remaining > 0 {
        return None;
    }
    let reset: i64 = headers
        .get("x-ratelimit-reset")?
        .to_str()
        .ok()?
        .parse()
        .ok()?;
    let at = chrono::DateTime::from_timestamp(reset, 0)?.with_timezone(&chrono::Local);
    Some(format!(
        "（未登录接口按出口 IP 限 60 次/小时，公司网络是整个办公室共用这个额度；{} 恢复）",
        at.format("%H:%M")
    ))
}

/// 取 JSON，失败时带回具体原因。
///
/// GitHub 出错会把原因写在 body 的 `message` 里（403 是 "API rate limit
/// exceeded for …"，404 是 "Not Found"），照抄它比自己猜准得多。
async fn fetch_json(c: &reqwest::Client, url: &str) -> Result<serde_json::Value, String> {
    let res = c.get(url).send().await.map_err(|e| describe_error(&e))?;
    let status = res.status();
    // 限流信息只在响应头里，得在 text() 把响应吃掉之前取出来。
    let reset_hint = rate_limit_reset_hint(res.headers());
    let body = res.text().await.map_err(|e| describe_error(&e))?;
    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("HTTP {} 但响应不是 JSON：{e}", status.as_u16()))?;
    if !status.is_success() {
        let msg = json.get("message").and_then(|v| v.as_str());
        let mut out = match msg.map(sanitize_api_message) {
            Some(m) if !m.is_empty() => format!("HTTP {}：{m}", status.as_u16()),
            _ => format!("HTTP {}", status.as_u16()),
        };
        if let Some(hint) = reset_hint {
            out.push_str(&hint);
        }
        return Err(out);
    }
    Ok(json)
}

// ---------------------------------------------------------------------------
// 搜索结果条目
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NpmItem {
    pub name: String,
    pub version: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub publisher: String,
    pub date: String,
    /// `official`（@deepseek-ai/ scope）或 `community`
    pub scope: String,
    pub homepage: String,
    pub repo_url: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GithubItem {
    pub repo: String,
    pub owner: String,
    pub name: String,
    pub description: String,
    pub stars: u64,
    pub forks: u64,
    pub language: String,
    pub updated: String,
    pub topics: Vec<String>,
    pub url: String,
}

/// 两种来源的条目形状不同，渲染层按来源取字段，所以这里不加 tag 直接摊平。
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(untagged)]
pub enum MarketItem {
    Npm(NpmItem),
    Github(GithubItem),
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub items: Vec<MarketItem>,
    pub has_more: bool,
    pub rate_limited: bool,
    pub total: usize,
}

fn npm_item(p: &serde_json::Value) -> Option<NpmItem> {
    let name = p.get("name")?.as_str()?.to_string();
    let links = p.get("links");
    let homepage = links
        .and_then(|l| l.get("homepage"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let repository = links
        .and_then(|l| l.get("repository"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    Some(NpmItem {
        scope: if name.starts_with("@deepseek-ai/") {
            "official".into()
        } else {
            "community".into()
        },
        version: p
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        description: p
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        keywords: p
            .get("keywords")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|k| k.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        publisher: p
            .get("publisher")
            .and_then(|v| v.get("username"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        date: p
            .get("date")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .chars()
            .take(10)
            .collect(),
        // 仓库地址取不到时回退到主页（渲染层用它显示"GitHub 页面"链接）。
        repo_url: {
            let from_repo = normalize_github_url(repository);
            if from_repo.is_empty() {
                normalize_github_url(&homepage)
            } else {
                from_repo
            }
        },
        homepage,
        name,
    })
}

fn github_item(it: &serde_json::Value) -> Option<GithubItem> {
    Some(GithubItem {
        repo: it.get("full_name")?.as_str()?.to_string(),
        owner: it
            .get("owner")
            .and_then(|o| o.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        name: it
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        description: it
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        stars: it
            .get("stargazers_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        forks: it.get("forks_count").and_then(|v| v.as_u64()).unwrap_or(0),
        language: it
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        updated: it
            .get("updated_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .chars()
            .take(10)
            .collect(),
        topics: it
            .get("topics")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|t| t.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        url: it
            .get("html_url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

// ---------------------------------------------------------------------------
// 分页游标
// ---------------------------------------------------------------------------

fn npm_queries(q: &str) -> Vec<String> {
    let mut list = Vec::new();
    if !q.is_empty() {
        list.push(q.to_string());
    }
    list.push("keywords:dsh-plugin".into());
    list.push("keywords:dsh".into());
    list
}

fn github_queries(q: &str) -> Vec<String> {
    let mut list = Vec::new();
    if !q.is_empty() {
        list.push(q.to_string());
    }
    list.push("dsh plugin".into());
    list.push("deepseek-harness plugin".into());
    list.push("topic:dsh".into());
    list
}

#[derive(Debug, Clone, Copy)]
struct Cursor {
    next: usize,
    exhausted: bool,
}

struct MarketState {
    source: String,
    query: String,
    seen: HashSet<String>,
    cursors: HashMap<String, Cursor>,
    rate_limited: bool,
}

static STATE: Mutex<Option<MarketState>> = Mutex::new(None);

/// 只在测试里用：把游标清空，避免用例之间互相影响。
#[cfg(test)]
fn reset_state() {
    *STATE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// 取出（必要时重建）当前搜索状态。
///
/// 不用 `expect`：`search_page` 会在 await 之间多次重新加锁，用 unwrap 一旦哪天
/// 状态被别处清空就是一次 panic。这里缺了就按当前参数重建，最差的后果是游标重置，
/// 而不是整个应用挂掉。
fn ensure_state<'a>(
    guard: &'a mut Option<MarketState>,
    source: &str,
    query: &str,
) -> &'a mut MarketState {
    guard.get_or_insert_with(|| MarketState {
        source: source.to_string(),
        query: query.to_string(),
        seen: HashSet::new(),
        cursors: HashMap::new(),
        rate_limited: false,
    })
}

async fn fetch_npm_page(query: &str, from: usize) -> Option<Vec<serde_json::Value>> {
    let url = format!(
        "https://registry.npmjs.org/-/v1/search?text={}&size={NPM_PAGE_SIZE}&from={from}",
        urlencode(query)
    );
    let res = client()?.get(url).send().await.ok()?;
    if !res.status().is_success() {
        return None;
    }
    let j: serde_json::Value = res.json().await.ok()?;
    Some(
        j.get("objects")?
            .as_array()?
            .iter()
            .filter_map(|o| o.get("package").cloned())
            .collect(),
    )
}

async fn fetch_github_page(query: &str, page: usize) -> Option<Vec<serde_json::Value>> {
    let url = format!(
        "https://api.github.com/search/repositories?q={}&per_page={GH_PAGE_SIZE}&page={page}&sort=stars",
        urlencode(query)
    );
    let res = client()?.get(url).send().await.ok()?;
    if !res.status().is_success() {
        return None; // 403 限流或其它错误
    }
    let j: serde_json::Value = res.json().await.ok()?;
    Some(j.get("items")?.as_array()?.clone())
}

/// 最小可用的 URL 查询值编码（只保留 unreserved 字符，其余按 %XX 转义）。
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 游标式翻页搜索。源或关键词变化（或 `reset`）就重置游标。
///
/// 每次最多返回 `MARKET_BATCH` 条；`has_more` 供渲染层决定是否还能继续滚动加载。
pub async fn search_page(
    source: &str,
    query: &str,
    reset: bool,
    core_packages: &HashSet<String>,
) -> SearchResult {
    let q = query.trim().to_string();
    let queries = if source == "github" {
        github_queries(&q)
    } else {
        npm_queries(&q)
    };

    // 需要重置就重置。注意：不能在整个 await 期间持锁，所以每次读写都短持有。
    {
        let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
        let need_reset = reset
            || guard
                .as_ref()
                .map(|s| s.source != source || s.query != q)
                .unwrap_or(true);
        if need_reset {
            *guard = Some(MarketState {
                source: source.to_string(),
                query: q.clone(),
                seen: HashSet::new(),
                cursors: HashMap::new(),
                rate_limited: false,
            });
        }
    }

    let mut items: Vec<MarketItem> = Vec::new();
    let mut rate_limited_now = false;

    for key in &queries {
        // 取当前游标（短持锁）。
        let cur = {
            let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
            let st = ensure_state(&mut guard, source, &q);
            *st.cursors.entry(key.clone()).or_insert(Cursor {
                next: if source == "github" { 1 } else { 0 },
                exhausted: false,
            })
        };
        if cur.exhausted {
            continue;
        }

        let raw = if source == "github" {
            fetch_github_page(key, cur.next).await
        } else {
            fetch_npm_page(key, cur.next).await
        };

        let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
        let st = ensure_state(&mut guard, source, &q);

        let Some(raw) = raw else {
            // 网络错误或限流：这个 query 本次到此为止。
            st.cursors.insert(
                key.clone(),
                Cursor {
                    next: cur.next,
                    exhausted: true,
                },
            );
            rate_limited_now = true;
            continue;
        };
        if raw.is_empty() {
            st.cursors.insert(
                key.clone(),
                Cursor {
                    next: cur.next,
                    exhausted: true,
                },
            );
            continue;
        }

        let mut next = cur.next + if source == "github" { 1 } else { raw.len() };
        let mut exhausted = false;
        if (source == "github" && next > GH_MAX_PAGE)
            || (source != "github" && next >= NPM_MAX_FROM)
        {
            exhausted = true;
            next = cur.next;
        }
        st.cursors.insert(key.clone(), Cursor { next, exhausted });

        for p in &raw {
            let id = if source == "github" {
                p.get("full_name").and_then(|v| v.as_str())
            } else {
                p.get("name").and_then(|v| v.as_str())
            };
            let Some(id) = id else { continue };
            if st.seen.contains(id) {
                continue;
            }
            st.seen.insert(id.to_string());

            if source == "github" {
                if let Some(item) = github_item(p) {
                    items.push(MarketItem::Github(item));
                }
            } else if let Some(item) = npm_item(p) {
                // npm：排除 dsh CLI 自身的核心包，且要求关键词命中 dsh/deepseek/harness。
                if core_packages.contains(&item.name) {
                    continue;
                }
                if !item.keywords.iter().any(|k| {
                    let k = k.to_ascii_lowercase();
                    k.contains("dsh") || k.contains("deepseek") || k.contains("harness")
                }) {
                    continue;
                }
                items.push(MarketItem::Npm(item));
            }
            if items.len() >= MARKET_BATCH {
                break;
            }
        }
        if items.len() >= MARKET_BATCH {
            break;
        }
    }

    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let st = ensure_state(&mut guard, source, &q);
    st.rate_limited = st.rate_limited || rate_limited_now;
    let has_more = items.len() >= MARKET_BATCH
        || queries
            .iter()
            .any(|k| st.cursors.get(k).map(|c| !c.exhausted).unwrap_or(false));
    SearchResult {
        items,
        has_more,
        rate_limited: st.rate_limited,
        total: st.seen.len(),
    }
}

// ---------------------------------------------------------------------------
// 详情
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RepoStats {
    pub full_name: String,
    pub stars: u64,
    pub forks: u64,
    pub open_issues: u64,
    pub created: String,
    pub pushed: String,
    pub license: Option<String>,
    pub description: String,
    pub topics: Vec<String>,
    pub readme: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NpmInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub license: Option<String>,
    pub homepage: String,
    pub repository: String,
    pub created: String,
    pub modified: String,
    pub downloads: Option<u64>,
    pub readme: String,
    pub repo: Option<RepoStats>,
    pub maintainers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GithubInfo {
    pub repo: String,
    pub name: String,
    pub pkg_name: Option<String>,
    pub version: String,
    pub description: String,
    pub stats: Option<RepoStats>,
    /// `stats` 为空时的具体原因，给界面和日志用。
    pub error: Option<String>,
}

const README_LIMIT: usize = 60_000;

/// 按字符数截断（不能按字节切，中文 README 会切出无效 UTF-8）。
fn truncate_chars(s: &str, limit: usize) -> String {
    s.chars().take(limit).collect()
}

/// 抓仓库活跃度。失败时带回原因，调用方自己决定是显示还是忽略。
pub async fn github_repo_stats(owner: &str, repo: &str) -> Result<RepoStats, String> {
    // IPC 层的参数是 Option，没传到就成了空串；空 owner/repo 拼出来的 /repos//
    // 只会换回一个 404，报成"仓库不存在"就把真正的问题盖住了。
    if owner.is_empty() || repo.is_empty() {
        return Err("缺少仓库标识（owner/repo）".into());
    }
    let c = client().ok_or("创建 HTTP 客户端失败")?;
    let g = fetch_json(&c, &format!("https://api.github.com/repos/{owner}/{repo}")).await?;

    // README 拉不到不影响其余信息。
    let mut readme = String::new();
    if let Ok(r) = c
        .get(format!(
            "https://raw.githubusercontent.com/{owner}/{repo}/HEAD/README.md"
        ))
        .send()
        .await
    {
        if r.status().is_success() {
            if let Ok(text) = r.text().await {
                readme = truncate_chars(&text, README_LIMIT);
            }
        }
    }

    let s = |k: &str| -> String { g.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string() };
    let n = |k: &str| -> u64 { g.get(k).and_then(|v| v.as_u64()).unwrap_or(0) };
    Ok(RepoStats {
        full_name: s("full_name"),
        stars: n("stargazers_count"),
        forks: n("forks_count"),
        open_issues: n("open_issues_count"),
        created: s("created_at").chars().take(10).collect(),
        pushed: s("pushed_at").chars().take(10).collect(),
        license: g
            .get("license")
            .and_then(|l| l.get("spdx_id"))
            .and_then(|v| v.as_str())
            .map(String::from),
        description: s("description"),
        topics: g
            .get("topics")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|t| t.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        readme,
    })
}

pub async fn npm_plugin_info(name: &str) -> Option<NpmInfo> {
    let c = client()?;
    let res = c
        .get(format!("https://registry.npmjs.org/{}", urlencode(name)))
        .send()
        .await
        .ok()?;
    if !res.status().is_success() {
        return None;
    }
    let pack: serde_json::Value = res.json().await.ok()?;
    if pack.get("error").is_some() {
        return None;
    }

    let latest = pack
        .get("dist-tags")
        .and_then(|t| t.get("latest"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let v = pack.get("versions").and_then(|vs| vs.get(&latest));

    // 周下载量拿不到就留空。
    let downloads = async {
        let r = c
            .get(format!(
                "https://api.npmjs.org/downloads/point/last-week/{}",
                urlencode(name)
            ))
            .send()
            .await
            .ok()?;
        let j: serde_json::Value = r.json().await.ok()?;
        j.get("downloads")?.as_u64()
    }
    .await;

    let repo_url = v
        .and_then(|v| v.get("repository"))
        .and_then(|r| r.get("url"))
        .and_then(|u| u.as_str())
        .unwrap_or("")
        .to_string();
    // 从仓库地址反查 GitHub 活跃度（分析档案里要用）。
    let repo = match parse_owner_repo(&repo_url) {
        Some((o, r)) => github_repo_stats(&o, &r).await.ok(),
        None => None,
    };

    Some(NpmInfo {
        name: pack
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(name)
            .to_string(),
        description: pack
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        keywords: pack
            .get("keywords")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|k| k.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        // license 可能是字符串，也可能是 { type: "MIT" } 这种老写法。
        license: v.and_then(|v| v.get("license")).and_then(|l| {
            l.as_str()
                .map(String::from)
                .or_else(|| l.get("type").and_then(|t| t.as_str()).map(String::from))
        }),
        homepage: v
            .and_then(|v| v.get("homepage"))
            .and_then(|v| v.as_str())
            .or_else(|| pack.get("homepage").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string(),
        created: pack
            .get("time")
            .and_then(|t| t.get("created"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .chars()
            .take(10)
            .collect(),
        modified: pack
            .get("time")
            .and_then(|t| t.get(&latest))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .chars()
            .take(10)
            .collect(),
        readme: truncate_chars(
            pack.get("readme").and_then(|v| v.as_str()).unwrap_or(""),
            README_LIMIT,
        ),
        maintainers: pack
            .get("maintainers")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(String::from))
                    .take(5)
                    .collect()
            })
            .unwrap_or_default(),
        repository: repo_url,
        version: latest,
        downloads,
        repo,
    })
}

/// 从形如 `git+https://github.com/o/r.git` 的地址里取 (owner, repo)。
pub fn parse_owner_repo(url: &str) -> Option<(String, String)> {
    let clean = normalize_github_url(url);
    let tail = clean.strip_prefix("https://github.com/")?;
    let mut parts = tail.split('/');
    let owner = parts.next()?.to_string();
    let repo = parts.next()?.to_string();
    if owner.is_empty() || repo.is_empty() {
        None
    } else {
        Some((owner, repo))
    }
}

pub async fn github_plugin_info(owner: &str, repo: &str) -> GithubInfo {
    let (stats, error) = match github_repo_stats(owner, repo).await {
        Ok(s) => (Some(s), None),
        Err(e) => (None, Some(e)),
    };

    // 仓库根的 package.json 用来拿真实包名（装 GitHub 插件时 allowBuilds 要用）。
    let mut pkg: Option<serde_json::Value> = None;
    if let Some(c) = client() {
        if let Ok(r) = c
            .get(format!(
                "https://raw.githubusercontent.com/{owner}/{repo}/HEAD/package.json"
            ))
            .send()
            .await
        {
            if r.status().is_success() {
                pkg = r.json().await.ok();
            }
        }
    }

    let pkg_name = pkg
        .as_ref()
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .map(String::from);
    GithubInfo {
        repo: format!("{owner}/{repo}"),
        name: pkg_name.clone().unwrap_or_else(|| repo.to_string()),
        version: pkg
            .as_ref()
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str())
            .unwrap_or("git")
            .to_string(),
        description: stats
            .as_ref()
            .map(|s| s.description.clone())
            .filter(|d| !d.is_empty())
            .or_else(|| {
                pkg.as_ref()
                    .and_then(|p| p.get("description"))
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
            .unwrap_or_default(),
        pkg_name,
        stats,
        error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencode_escapes_query_values() {
        assert_eq!(urlencode("keywords:dsh"), "keywords%3Adsh");
        assert_eq!(urlencode("dsh plugin"), "dsh%20plugin");
        assert_eq!(urlencode("@deepseek-ai/dsh"), "%40deepseek-ai%2Fdsh");
        assert_eq!(urlencode("a-b_c.d~e"), "a-b_c.d~e");
    }

    #[test]
    fn truncate_is_char_safe() {
        // 按字节切会切出无效 UTF-8；这里必须按字符。
        let s = "中文字符测试";
        assert_eq!(truncate_chars(s, 3), "中文字");
        assert_eq!(truncate_chars(s, 100), s);
    }

    #[test]
    fn query_lists_match_electron_version() {
        assert_eq!(
            npm_queries("foo"),
            vec!["foo", "keywords:dsh-plugin", "keywords:dsh"]
        );
        // 空关键词时只留兜底 query。
        assert_eq!(npm_queries("").len(), 2);
        assert_eq!(github_queries("").len(), 3);
        assert!(github_queries("bar")[0] == "bar");
    }

    #[test]
    fn parses_owner_repo_from_various_urls() {
        assert_eq!(
            parse_owner_repo("git+https://github.com/o/r.git"),
            Some(("o".into(), "r".into()))
        );
        assert_eq!(
            parse_owner_repo("https://github.com/owner/repo/tree/main"),
            Some(("owner".into(), "repo".into()))
        );
        assert_eq!(parse_owner_repo("https://npmjs.com/x"), None);
        assert_eq!(parse_owner_repo(""), None);
    }

    #[test]
    fn npm_item_maps_scope_and_repo_url() {
        let p: serde_json::Value = serde_json::from_str(
            r#"{"name":"@deepseek-ai/x","version":"1.0.0","description":"d",
                "keywords":["dsh"],"publisher":{"username":"u"},
                "date":"2026-01-02T00:00:00Z",
                "links":{"repository":"git+https://github.com/o/r.git","homepage":""}}"#,
        )
        .unwrap();
        let it = npm_item(&p).unwrap();
        assert_eq!(it.scope, "official");
        assert_eq!(it.date, "2026-01-02");
        assert_eq!(it.repo_url, "https://github.com/o/r");

        // 没有仓库信息时回退到主页。
        let p2: serde_json::Value = serde_json::from_str(
            r#"{"name":"plain","links":{"repository":"","homepage":"https://github.com/a/b"}}"#,
        )
        .unwrap();
        let it2 = npm_item(&p2).unwrap();
        assert_eq!(it2.scope, "community");
        assert_eq!(it2.repo_url, "https://github.com/a/b");
    }

    #[test]
    fn github_item_maps_fields() {
        let it: serde_json::Value = serde_json::from_str(
            r#"{"full_name":"o/r","owner":{"login":"o"},"name":"r","description":"d",
                "stargazers_count":5,"forks_count":2,"language":"Rust",
                "updated_at":"2026-01-02T00:00:00Z","topics":["dsh"],
                "html_url":"https://github.com/o/r"}"#,
        )
        .unwrap();
        let g = github_item(&it).unwrap();
        assert_eq!(g.stars, 5);
        assert_eq!(g.updated, "2026-01-02");
        assert_eq!(g.url, "https://github.com/o/r");
    }

    /// 渲染层（market.js）按名字取这些字段，改名会让市场卡片静默空白。
    #[test]
    fn serialized_keys_match_renderer_expectations() {
        let v = serde_json::to_value(MarketItem::Npm(NpmItem {
            name: "x".into(),
            version: "1".into(),
            description: "d".into(),
            keywords: vec![],
            publisher: "p".into(),
            date: "2026-01-01".into(),
            scope: "community".into(),
            homepage: String::new(),
            repo_url: "https://github.com/o/r".into(),
        }))
        .unwrap();
        for k in ["name", "version", "description", "date", "scope", "repoUrl"] {
            assert!(v.get(k).is_some(), "npm 条目缺少 {k}: {v}");
        }
        assert!(v.get("repo_url").is_none(), "不该有 snake_case 残留");

        let v = serde_json::to_value(MarketItem::Github(GithubItem {
            repo: "o/r".into(),
            owner: "o".into(),
            name: "r".into(),
            description: "d".into(),
            stars: 1,
            forks: 0,
            language: "Rust".into(),
            updated: "2026-01-01".into(),
            topics: vec![],
            url: "https://github.com/o/r".into(),
        }))
        .unwrap();
        for k in ["repo", "name", "stars", "language", "updated", "url"] {
            assert!(v.get(k).is_some(), "github 条目缺少 {k}: {v}");
        }

        let v = serde_json::to_value(SearchResult::default()).unwrap();
        for k in ["items", "hasMore", "rateLimited", "total"] {
            assert!(v.get(k).is_some(), "搜索结果缺少 {k}: {v}");
        }
    }

    /// 三项市场行为合并成一个顺序用例。
    ///
    /// 它们共享同一个全局分页游标，分成三个 `#[tokio::test]` 会被并行调度、
    /// 互相消耗页码（还会把彼此的状态 reset 掉），断言就失去意义了。
    #[tokio::test]
    async fn npm_search_filters_paginates_and_excludes_core() {
        reset_state();
        let core = HashSet::new();
        let first = search_page("npm", "", true, &core).await;
        if first.rate_limited || first.items.is_empty() {
            return; // 网络受限时不作断言
        }

        // ① 过滤条件：只返回 npm 条目，且关键词命中 dsh/deepseek/harness。
        for it in &first.items {
            match it {
                MarketItem::Npm(n) => {
                    assert!(!n.name.is_empty());
                    assert!(
                        n.keywords.iter().any(|k| {
                            let k = k.to_ascii_lowercase();
                            k.contains("dsh") || k.contains("deepseek") || k.contains("harness")
                        }),
                        "{} 的关键词没命中过滤条件: {:?}",
                        n.name,
                        n.keywords
                    );
                }
                MarketItem::Github(_) => panic!("npm 源不该返回 GitHub 条目"),
            }
        }

        // ② 分页去重：第二批不该重复第一批的条目。
        let ids = |r: &SearchResult| -> HashSet<String> {
            r.items
                .iter()
                .map(|i| match i {
                    MarketItem::Npm(n) => n.name.clone(),
                    MarketItem::Github(g) => g.repo.clone(),
                })
                .collect()
        };
        let a = ids(&first);
        let second = search_page("npm", "", false, &core).await;
        let b = ids(&second);
        assert!(
            a.is_disjoint(&b),
            "分页出现重复条目: {:?}",
            a.intersection(&b).collect::<Vec<_>>()
        );

        // ③ 核心包排除：把第一批里的某个包当成 dsh CLI 自身依赖，它就不该再出现。
        let victim = match first.items.first().unwrap() {
            MarketItem::Npm(n) => n.name.clone(),
            MarketItem::Github(g) => g.repo.clone(),
        };
        let mut core2 = HashSet::new();
        core2.insert(victim.clone());
        reset_state();
        let filtered = search_page("npm", "", true, &core2).await;
        for it in &filtered.items {
            if let MarketItem::Npm(n) = it {
                assert_ne!(n.name, victim, "被列为核心包的 {victim} 应被排除");
            }
        }
    }

    #[tokio::test]
    async fn unknown_npm_package_info_is_none() {
        assert!(npm_plugin_info("@modred522/definitely-not-real-xyz")
            .await
            .is_none());
    }

    /// 空 owner/repo 必须在发请求之前挡下，并且说清是参数缺失 —— 让它拼出
    /// /repos// 去换一个 404，就会被报成"仓库不存在"，把 IPC 参数没传到的
    /// 真问题盖掉。
    #[tokio::test]
    async fn empty_owner_repo_reports_missing_ref_not_network_error() {
        let e = github_repo_stats("", "").await.unwrap_err();
        assert!(e.contains("缺少仓库标识"), "原因不对: {e}");
        let info = github_plugin_info("", "deepseek-harness").await;
        assert!(info.stats.is_none());
        assert!(
            info.error.as_deref().unwrap_or("").contains("缺少仓库标识"),
            "error 字段没带上原因: {:?}",
            info.error
        );
    }

    /// 渲染层靠 `error` 决定显示具体原因还是泛泛的失败提示。
    #[test]
    fn github_info_serializes_error_field() {
        let v = serde_json::to_value(GithubInfo {
            repo: "o/r".into(),
            name: "r".into(),
            pkg_name: None,
            version: "git".into(),
            description: String::new(),
            stats: None,
            error: Some("HTTP 403：API rate limit exceeded".into()),
        })
        .unwrap();
        assert_eq!(
            v.get("error").and_then(|e| e.as_str()),
            Some("HTTP 403：API rate limit exceeded")
        );
        assert!(v.get("pkgName").is_some(), "应为 camelCase: {v}");
    }

    /// 连接失败必须带上根因。打 127.0.0.1:1（没人监听）离线就能复现。
    ///
    /// 这条断言的是"报错说得清不清"，不是"网络通不通" —— 以前所有网络失败
    /// 都被压成 None，界面只能说"可能被限流或仓库不存在"。
    #[tokio::test]
    async fn connect_failure_is_described_with_root_cause() {
        let c = client().unwrap();
        let e = c.get("http://127.0.0.1:1/").send().await.unwrap_err();
        let msg = describe_error(&e);
        assert!(msg.starts_with("连接失败"), "分类不对: {msg}");
        // 根因（拒绝连接之类）得真在里面，只有分类名等于什么都没说。
        assert!(
            msg.chars().count() > "连接失败：".chars().count() + 5,
            "没带根因: {msg}"
        );
    }

    /// 非 2xx 要把状态码和接口自己给的原因一起带出来。
    ///
    /// 用一个必定不存在的仓库触发 404；万一 CI 那边正被限流，返回的是 403
    /// 加"API rate limit exceeded"，形状一样 —— 断言的是格式，不是具体码。
    #[tokio::test]
    async fn http_error_reports_status_and_api_message() {
        let c = client().unwrap();
        let r = fetch_json(
            &c,
            "https://api.github.com/repos/modred522/definitely-not-real-xyz",
        )
        .await;
        let Err(e) = r else {
            panic!("不存在的仓库不该成功");
        };
        if e.starts_with("连接失败") || e.starts_with("请求超时") {
            return; // 无外网环境，跳过
        }
        assert!(e.starts_with("HTTP "), "缺状态码: {e}");
        assert!(e.contains('：'), "没把接口给的原因带出来: {e}");
    }

    /// 限流提示里的公网出口 IP 不能原样写进日志或界面。
    #[test]
    fn api_message_masks_ip_and_drops_boilerplate() {
        let raw = "API rate limit exceeded for 103.126.92.187. (But here's the good news: \
                   Authenticated requests get a higher rate limit. Check out the documentation \
                   for more details.)";
        assert_eq!(
            sanitize_api_message(raw),
            "API rate limit exceeded for <出口 IP>.",
            "括号里的推销该整段砍掉，不该留半截话"
        );

        // 没有括号可砍时才按长度截，并且要让人看出来是被截的。
        let cut = sanitize_api_message(&"x".repeat(200));
        assert_eq!(cut.chars().count(), 121, "{cut}");
        assert!(cut.ends_with('…'), "{cut}");

        // 版本号之类的不是 IP，别乱改。
        assert_eq!(sanitize_api_message("Not Found"), "Not Found");
        assert_eq!(sanitize_api_message("bad v1.2.3 x"), "bad v1.2.3 x");
        assert_eq!(mask_ipv4("127.0.0.1"), "<出口 IP>");
    }

    /// 配额耗尽才提限流，还有余量的 403 是别的原因（例如缺 User-Agent）。
    #[test]
    fn rate_limit_hint_only_when_quota_is_gone() {
        use reqwest::header::{HeaderMap, HeaderValue};
        let mut h = HeaderMap::new();
        assert!(rate_limit_reset_hint(&h).is_none(), "没有头就别猜");

        h.insert("x-ratelimit-remaining", HeaderValue::from_static("0"));
        h.insert("x-ratelimit-reset", HeaderValue::from_static("1789541888"));
        let hint = rate_limit_reset_hint(&h).expect("配额耗尽时要给出恢复时间");
        assert!(hint.contains("60 次/小时"), "{hint}");
        assert!(hint.contains("恢复"), "{hint}");

        h.insert("x-ratelimit-remaining", HeaderValue::from_static("42"));
        assert!(
            rate_limit_reset_hint(&h).is_none(),
            "还有 42 次余量，不该报成限流"
        );
    }

    /// 代理地址可能带着用户名密码，报错时不能原样写进日志。
    #[test]
    fn proxy_url_credentials_are_masked() {
        assert_eq!(
            mask_credentials("http://alice:s3cret@proxy.corp:8080"),
            "http://proxy.corp:8080"
        );
        assert_eq!(
            mask_credentials("http://127.0.0.1:7890"),
            "http://127.0.0.1:7890"
        );
        assert_eq!(mask_credentials(""), "");
    }
}
