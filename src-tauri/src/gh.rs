//! GitHub / HTTP 管道：客户端、令牌闸门、错误诊断、限流提示。
//!
//! 单独成一个模块是因为**插件市场和更新检查都要用**。放在 `market.rs` 里的时候，
//! `updates.rs` 自己另建了一个 reqwest client —— 于是那条路既拿不到令牌，
//! 也没有诊断，changelog 抓不到就只剩一个 `None`。
//!
//! 这里有一条不能破的线：**令牌只发给 `api.github.com`**，见 `token_for`。
//! 详见 `token.rs` 的模块注释。

use std::time::Duration;

/// 唯一允许带令牌的前缀。带尾斜杠是关键：`https://api.github.com.evil.com/`
/// 和 `https://api.github.com@evil.com/` 都不以它开头，匹配不上。
const GH_API: &str = "https://api.github.com/";

fn is_github_api(url: &str) -> bool {
    url.starts_with(GH_API)
}

/// 这个请求该用哪个令牌 —— **白名单之外一律 `None`**。
///
/// 这是唯一一道闸门：要带令牌的判断只写在这里一处，`with_token` 只管照着贴。
/// 早先的写法在 `fetch_json` 里又算了一遍同样的条件，改一处漏一处就会让
/// 限流提示说反话。
///
/// **不能图省事塞进 `default_headers`**：`client()` 同时被 npm registry 的请求
/// 用着，那等于把 GitHub 凭据递给第三方主机。这里按 URL 主机名判定，
/// 靠数据保证而不是靠调用方自觉。
///
/// `raw.githubusercontent.com` 也故意不带 —— 公开仓库的 README / package.json
/// 不需要认证，少一个地方接触凭据就少一份风险。
///
/// 跨主机重定向不用担心：reqwest 的 `remove_sensitive_headers` 会在换主机时
/// 摘掉 `Authorization`（已核对 reqwest 0.12 的 redirect.rs）。
fn token_for(url: &str) -> Option<String> {
    if !is_github_api(url) {
        return None;
    }
    crate::token::load()
}

fn with_token(req: reqwest::RequestBuilder, token: Option<&str>) -> reqwest::RequestBuilder {
    match token {
        Some(t) => req.bearer_auth(t),
        None => req,
    }
}

pub(crate) fn client() -> Option<reqwest::Client> {
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
fn rate_limit_reset_hint(headers: &reqwest::header::HeaderMap, authorized: bool) -> Option<String> {
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
    let when = at.format("%H:%M");
    Some(if authorized {
        format!("（已带令牌，5000 次/小时的额度也用光了；{when} 恢复）")
    } else {
        format!(
            "（未登录接口按出口 IP 限 60 次/小时，公司网络是整个办公室共用这个额度；             {when} 恢复。设置里配一个 GitHub 令牌可提到 5000 次/小时）"
        )
    })
}

/// 取 JSON，失败时带回具体原因。
///
/// GitHub 出错会把原因写在 body 的 `message` 里（403 是 "API rate limit
/// exceeded for …"，404 是 "Not Found"），照抄它比自己猜准得多。
pub(crate) async fn fetch_json(
    c: &reqwest::Client,
    url: &str,
) -> Result<serde_json::Value, String> {
    let token = token_for(url);
    let res = with_token(c.get(url), token.as_deref())
        .send()
        .await
        .map_err(|e| describe_error(&e))?;
    let status = res.status();
    // 限流信息只在响应头里，得在 text() 把响应吃掉之前取出来。
    let reset_hint = rate_limit_reset_hint(res.headers(), token.is_some());
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

/// 用给定令牌探一次 `/rate_limit`，回传 core 额度上限。
///
/// 存之前先验：无效令牌存进去，市场只会继续以"限流"的面目失败，用户根本
/// 想不到是令牌打错了。
///
/// 这里直接 `bearer_auth` 而没过 `token_for()` —— 因为待验的令牌还没保存，
/// `token::load()` 取不到。URL 是写死的 `{GH_API}rate_limit`，仍在白名单内。
pub(crate) async fn probe_token(token: &str) -> Result<u64, String> {
    let c = client().ok_or("创建 HTTP 客户端失败")?;
    let url = format!("{GH_API}rate_limit");
    debug_assert!(is_github_api(&url));
    let res = c
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| describe_error(&e))?;
    let status = res.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err("令牌无效或已过期（HTTP 401）".into());
    }
    let body = res.text().await.map_err(|e| describe_error(&e))?;
    let j: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("HTTP {} 但响应不是 JSON：{e}", status.as_u16()))?;
    if !status.is_success() {
        let m = j
            .get("message")
            .and_then(|v| v.as_str())
            .map(sanitize_api_message);
        return Err(match m {
            Some(m) if !m.is_empty() => format!("HTTP {}：{m}", status.as_u16()),
            _ => format!("HTTP {}", status.as_u16()),
        });
    }
    j.get("resources")
        .and_then(|r| r.get("core"))
        .and_then(|c| c.get("limit"))
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "响应里没有 core 额度字段".to_string())
}

/// 按字符数截断（不能按字节切，中文 README 会切出无效 UTF-8）。
pub(crate) fn truncate_chars(s: &str, limit: usize) -> String {
    s.chars().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_is_char_safe() {
        // 按字节切会切出无效 UTF-8；这里必须按字符。
        let s = "中文字符测试";
        assert_eq!(truncate_chars(s, 3), "中文字");
        assert_eq!(truncate_chars(s, 100), s);
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
        assert!(rate_limit_reset_hint(&h, false).is_none(), "没有头就别猜");

        h.insert("x-ratelimit-remaining", HeaderValue::from_static("0"));
        h.insert("x-ratelimit-reset", HeaderValue::from_static("1789541888"));
        let hint = rate_limit_reset_hint(&h, false).expect("配额耗尽时要给出恢复时间");
        assert!(hint.contains("60 次/小时"), "{hint}");
        assert!(hint.contains("恢复"), "{hint}");
        // 没带令牌时要告诉用户还有这条出路。
        assert!(hint.contains("5000"), "该提示配令牌可提额度: {hint}");

        // 带了令牌还被限，就不能再说"未登录 60 次/小时"了 —— 那是误导。
        let authed = rate_limit_reset_hint(&h, true).expect("带令牌也要给恢复时间");
        assert!(authed.contains("已带令牌"), "{authed}");
        assert!(!authed.contains("60 次/小时"), "口径串了: {authed}");

        h.insert("x-ratelimit-remaining", HeaderValue::from_static("42"));
        assert!(
            rate_limit_reset_hint(&h, false).is_none(),
            "还有 42 次余量，不该报成限流"
        );
    }

    /// 令牌只许发给 api.github.com。这条要是松了，等于把 GitHub 凭据
    /// 递给 npm registry 或者任意一个把主机名拼在前缀里的域名。
    #[test]
    fn token_host_gate_is_exact() {
        assert!(is_github_api("https://api.github.com/repos/o/r"));
        assert!(is_github_api(
            "https://api.github.com/search/repositories?q=x"
        ));

        // npm 侧的请求一个都不许带。
        assert!(!is_github_api("https://registry.npmjs.org/some-pkg"));
        assert!(!is_github_api("https://api.npmjs.org/downloads/point/x"));
        // 公开仓库的 raw 文件不需要认证，也不给。
        assert!(!is_github_api(
            "https://raw.githubusercontent.com/o/r/HEAD/README.md"
        ));
        // 把主机名拼进前缀的几种老套路。
        assert!(!is_github_api("https://api.github.com.evil.com/repos/o/r"));
        assert!(!is_github_api("https://api.github.com@evil.com/repos/o/r"));
        assert!(!is_github_api("https://evil.com/https://api.github.com/x"));
        // 明文 http 也不给：令牌不能走未加密连接。
        assert!(!is_github_api("http://api.github.com/repos/o/r"));
    }

    /// 形状合法但不存在的令牌，GitHub 会回 401。
    ///
    /// 这条同时证明了两件事：保存前的校验能挡住废令牌，以及 `Authorization`
    /// 头**确实发到了对面**而不是被我们自己吞掉 —— 否则拿到的会是 403 限流
    /// 或者 200，而不是 401。没有有效令牌也能验的办法。
    #[tokio::test]
    async fn probe_rejects_a_bogus_token() {
        let e = probe_token("ghp_00000000000000000000000000000000000000")
            .await
            .unwrap_err();
        if e.starts_with("连接失败") || e.starts_with("请求超时") {
            return; // 无外网环境，跳过
        }
        assert!(e.contains("401"), "假令牌应当被 401 拒掉，实际: {e}");
    }

    /// 白名单之外的主机，`token_for` 必须回 `None` —— 有没有令牌都一样。
    #[test]
    fn token_for_never_leaks_past_the_allowlist() {
        for u in [
            "https://registry.npmjs.org/some-pkg",
            "https://api.npmjs.org/downloads/point/last-week/x",
            "https://raw.githubusercontent.com/o/r/HEAD/README.md",
            "https://api.github.com.evil.com/repos/o/r",
            "http://api.github.com/repos/o/r",
        ] {
            assert!(token_for(u).is_none(), "{u} 不该拿到令牌");
        }
    }

    /// 没有令牌时不该凭空长出一个 Authorization 头。
    #[tokio::test]
    async fn no_auth_header_without_a_token() {
        let c = client().unwrap();
        let req = with_token(c.get(GH_API), None).build().unwrap();
        assert!(
            req.headers().get(reqwest::header::AUTHORIZATION).is_none(),
            "没令牌却带了 Authorization"
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
