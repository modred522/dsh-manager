//! 纯函数模块：无 Tauri / 文件系统 / 网络 / 全局状态依赖，可被 `cargo test` 直接测试。
//!
//! 这是 Electron 版 `lib/pure.js` 的 Rust 移植。行为与原实现逐条对齐（含 `test/pure.test.js`
//! 里那 16 条用例的期望），移植时刻意没有"顺手改进"，以免重构同时引入行为差异。

use std::cmp::Ordering;
use std::sync::LazyLock;

use regex::Regex;

// ---------------------------------------------------------------------------
// 版本号
// ---------------------------------------------------------------------------

/// 预发布段的一节：数字段或字符串段（semver 规定数字段之间按数值比，数字段 > 字符串段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreSeg {
    Num(u64),
    Str(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub nums: [u64; 3],
    /// `None` 表示正式版（无预发布段）。
    pub pre: Option<Vec<PreSeg>>,
}

/// 解析版本号。容忍前导 `v` 与首尾空白；核心段不足 3 节时补 0。
pub fn parse_version(v: &str) -> Version {
    let s = v.trim().trim_start_matches('v');
    let mut parts = s.splitn(2, '-');
    let core = parts.next().unwrap_or("");
    let pre_raw = parts.next();

    let mut nums = [0u64; 3];
    for (i, seg) in core.split('.').take(3).enumerate() {
        // 与 JS 的 `parseInt(n, 10) || 0` 对齐：解析失败记 0。
        nums[i] = leading_number(seg).unwrap_or(0);
    }

    let pre = pre_raw.map(|raw| {
        raw.split('.')
            .map(|p| match p.parse::<u64>() {
                Ok(n) if p.chars().all(|c| c.is_ascii_digit()) => PreSeg::Num(n),
                _ => PreSeg::Str(p.to_string()),
            })
            .collect()
    });

    Version { nums, pre }
}

/// 取字符串开头的十进制数字（对齐 JS `parseInt` 的宽松行为，如 "1abc" -> 1）。
fn leading_number(s: &str) -> Option<u64> {
    let digits: String = s
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

/// 按 semver 规则比较两个版本。
///
/// 例：`rc.10 > rc.9`（纯字符串比较会错）；`1.0.0 > 1.0.0-rc.1`。
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let x = parse_version(a);
    let y = parse_version(b);
    for i in 0..3 {
        if x.nums[i] != y.nums[i] {
            return x.nums[i].cmp(&y.nums[i]);
        }
    }
    cmp_pre(x.pre.as_deref(), y.pre.as_deref())
}

fn cmp_pre(xp: Option<&[PreSeg]>, yp: Option<&[PreSeg]>) -> Ordering {
    match (xp, yp) {
        (None, None) => Ordering::Equal,
        // 有预发布段的一方更小：1.0.0 > 1.0.0-rc.1。
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => {
            let len = x.len().max(y.len());
            for i in 0..len {
                match (x.get(i), y.get(i)) {
                    // 段数少的更小。
                    (None, Some(_)) => return Ordering::Less,
                    (Some(_), None) => return Ordering::Greater,
                    (Some(PreSeg::Num(a)), Some(PreSeg::Num(b))) => {
                        if a != b {
                            return a.cmp(b);
                        }
                    }
                    // 数字段 > 字符串段。
                    (Some(PreSeg::Num(_)), Some(PreSeg::Str(_))) => return Ordering::Greater,
                    (Some(PreSeg::Str(_)), Some(PreSeg::Num(_))) => return Ordering::Less,
                    (Some(PreSeg::Str(a)), Some(PreSeg::Str(b))) => {
                        if a != b {
                            return a.cmp(b);
                        }
                    }
                    (None, None) => unreachable!("i < len 保证至少一侧有值"),
                }
            }
            Ordering::Equal
        }
    }
}

/// 是否为预发布版本（用于在日志/通知/更新弹窗里标注）。
pub fn is_prerelease(v: &str) -> bool {
    parse_version(v).pre.is_some()
}

// ---------------------------------------------------------------------------
// 字符串清洗
// ---------------------------------------------------------------------------

static GITHUB_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"github\.com/([^/]+)/([^/?#.]+)").unwrap());

/// 从仓库/主页 URL 中提取干净的 GitHub 页面地址；提取不到返回空串。
pub fn normalize_github_url(url: &str) -> String {
    GITHUB_RE
        .captures(url)
        .map(|c| format!("https://github.com/{}/{}", &c[1], &c[2]))
        .unwrap_or_default()
}

static CIM_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{4})(\d{2})(\d{2})(\d{2})(\d{2})(\d{2})").unwrap());

/// WMI CreationDate（CIM datetime，形如 `20260820093405.123456+480`）→ `YYYY-MM-DD HH:MM:SS`。
pub fn parse_cim_date(s: &str) -> Option<String> {
    let c = CIM_RE.captures(s.trim())?;
    Some(format!(
        "{}-{}-{} {}:{}:{}",
        &c[1], &c[2], &c[3], &c[4], &c[5], &c[6]
    ))
}

struct ReleaseRes {
    h3_html: Regex,
    h3_md: Regex,
    tags: Regex,
    md_link: Regex,
    bold: Regex,
    bullet: Regex,
    code: Regex,
    rule: Regex,
    bracket: Regex,
}

static RELEASE_RES: LazyLock<ReleaseRes> = LazyLock::new(|| ReleaseRes {
    h3_html: Regex::new(r"(?is)<h3[^>]*>(.*?)</h3>").unwrap(),
    h3_md: Regex::new(r"(?m)^###\s+").unwrap(),
    tags: Regex::new(r"<[^>]+>").unwrap(),
    md_link: Regex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap(),
    bold: Regex::new(r"\*\*([^*]+)\*\*").unwrap(),
    bullet: Regex::new(r"(?m)^\s*[-*]\s+").unwrap(),
    code: Regex::new(r"`([^`]*)`").unwrap(),
    rule: Regex::new(r"(?m)^\s*---+\s*$").unwrap(),
    bracket: Regex::new(r"\[[^\]]*\]").unwrap(),
});

/// GitHub Release 正文 → 纯文本 changelog（只保留中文段，清理 markdown/HTML 标记）。
pub fn release_body_to_text(body: &str) -> String {
    let re = &*RELEASE_RES;
    let mut text = body.to_string();

    // 英文段以 `<h3 id="en">` 起头，从那里截断。
    if let Some(idx) = text.find(r#"<h3 id="en">"#) {
        text.truncate(idx);
    }

    text = re
        .h3_html
        .replace_all(&text, |c: &regex::Captures| {
            let inner = re.tags.replace_all(&c[1], "");
            let inner = re.bracket.replace_all(&inner, "");
            format!("\n■ {}\n", inner.trim())
        })
        .into_owned();
    text = re.h3_md.replace_all(&text, "\n■ ").into_owned();
    text = re.tags.replace_all(&text, "").into_owned();
    text = text
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    text = re.md_link.replace_all(&text, "$1").into_owned();
    text = re.bold.replace_all(&text, "$1").into_owned();
    text = re.bullet.replace_all(&text, "  • ").into_owned();
    text = re.code.replace_all(&text, "$1").into_owned();
    text = re.rule.replace_all(&text, "").into_owned();

    text.lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

static JSON_BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)\{.*?\}").unwrap());

/// 从模型输出里提取最后一个可解析的 JSON 评分卡（含 `score` 或 `verdict` 字段）。
pub fn extract_analysis_json(text: &str) -> Option<serde_json::Value> {
    let blocks: Vec<&str> = JSON_BLOCK_RE.find_iter(text).map(|m| m.as_str()).collect();
    for b in blocks.iter().rev() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(b) {
            if v.get("score").is_some() || v.get("verdict").is_some() {
                return Some(v);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 日志
// ---------------------------------------------------------------------------

/// 子进程输出 chunk → 逐行（去掉行尾空白与空行）。
///
/// 子进程的 chunk 自带结尾换行，直接写日志会让每条之间多一个空行；
/// 且一个 chunk 可能含多行，拆开后才能逐行做去重与打码。
pub fn split_log_chunk(chunk: &str) -> Vec<String> {
    chunk
        .split('\n')
        .map(|l| l.trim_end().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

const SECRET_KEYS: &str =
    r"tokens?|access[_-]?token|refresh[_-]?token|api[_-]?keys?|apikey|secret|password|passwd|pwd";

/// GitHub 令牌的已知前缀。`SECRET_KEY_RE` 只认 `key=value` 形式，裸令牌漏网；
/// 令牌本来就不该进日志（见 token.rs 铁律 2），这条是兜底。
const GITHUB_PAT_RE: &str = r"\b(gh[pousr]_|github_pat_)[A-Za-z0-9_]{10,}";

struct SecretRes {
    kv: Regex,
    json_kv: Regex,
    bearer: Regex,
    sk: Regex,
    pat: Regex,
}

static SECRET_RES: LazyLock<SecretRes> = LazyLock::new(|| SecretRes {
    // 不加 \b：形如 DEEPSEEK_API_KEY 的前缀会把词边界吃掉，宁可多匹配键名（只替换值，无副作用）。
    kv: Regex::new(&format!(r#"(?i)({SECRET_KEYS})=[^\s&"'`]+"#)).unwrap(),
    // 只匹配带引号的形式，避免把中文日志里的 "xxx: yyy" 误伤。
    json_kv: Regex::new(&format!(r#"(?i)"({SECRET_KEYS})"\s*:\s*"[^"]*""#)).unwrap(),
    bearer: Regex::new(r"(?i)\b(Bearer)\s+[A-Za-z0-9._~+/-]+=*").unwrap(),
    sk: Regex::new(r"\bsk-[A-Za-z0-9_-]{8,}").unwrap(),
    pat: Regex::new(GITHUB_PAT_RE).unwrap(),
});

/// 日志里的敏感值打码。
///
/// `dsh web` 启动时会把带 token 的地址打到 stdout，原样落盘就等于把 web 界面的
/// 访问凭据写进保留 7 天、还能一键导出的日志文件。
pub fn redact_secrets(line: &str) -> String {
    let re = &*SECRET_RES;
    let s = re.kv.replace_all(line, "$1=***");
    let s = re.json_kv.replace_all(&s, r#""$1": "***""#);
    let s = re.bearer.replace_all(&s, "$1 ***");
    let s = re.sk.replace_all(&s, "sk-***");
    re.pat.replace_all(&s, "$1***").into_owned()
}

// ---------------------------------------------------------------------------
// 测试：对齐 Electron 版 test/pure.test.js
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering::{Equal, Greater, Less};

    #[test]
    fn compare_versions_numeric_core() {
        assert_eq!(compare_versions("0.2.0", "0.1.9"), Greater);
        assert_eq!(compare_versions("0.1.9", "0.2.0"), Less);
        assert_eq!(compare_versions("1.0.0", "1.0.0"), Equal);
        assert_eq!(compare_versions("1.0.1", "1.0.0"), Greater);
        assert_eq!(compare_versions("0.1.0", "0.10.0"), Less);
    }

    #[test]
    fn compare_versions_prerelease_vs_release() {
        assert_eq!(compare_versions("1.0.0", "1.0.0-rc.1"), Greater);
        assert_eq!(compare_versions("1.0.0-rc.1", "1.0.0"), Less);
    }

    #[test]
    fn compare_versions_numeric_prerelease_identifiers() {
        // rc.10 > rc.9：纯字符串比较会判错，这是历史上真出过的 bug。
        assert_eq!(compare_versions("0.1.0-rc.10", "0.1.0-rc.9"), Greater);
        assert_eq!(compare_versions("0.1.0-rc.2", "0.1.0-rc.10"), Less);
    }

    #[test]
    fn compare_versions_mixed_prerelease() {
        // 数字段 > 字符串段。
        assert_eq!(compare_versions("1.0.0-1", "1.0.0-alpha"), Greater);
        // 段数少的更小。
        assert_eq!(compare_versions("1.0.0-rc", "1.0.0-rc.1"), Less);
        assert_eq!(compare_versions("0.1.6-alpha.1", "0.1.5-rc.2"), Greater);
    }

    #[test]
    fn compare_versions_tolerates_v_prefix_and_space() {
        assert_eq!(compare_versions(" v1.2.3 ", "1.2.3"), Equal);
    }

    #[test]
    fn parse_version_shapes() {
        let v = parse_version("1.2.3");
        assert_eq!(v.nums, [1, 2, 3]);
        assert!(v.pre.is_none());

        let v = parse_version("0.1.0-rc.7");
        assert_eq!(v.nums, [0, 1, 0]);
        assert_eq!(v.pre, Some(vec![PreSeg::Str("rc".into()), PreSeg::Num(7)]));

        // 核心段不足补 0。
        assert_eq!(parse_version("2").nums, [2, 0, 0]);
    }

    #[test]
    fn is_prerelease_flag() {
        assert!(is_prerelease("0.1.6-alpha.1"));
        assert!(!is_prerelease("1.0.0"));
    }

    #[test]
    fn normalize_github_url_variants() {
        assert_eq!(
            normalize_github_url("git+https://github.com/o/r.git"),
            "https://github.com/o/r"
        );
        assert_eq!(
            normalize_github_url("https://github.com/o/r/tree/main"),
            "https://github.com/o/r"
        );
        assert_eq!(normalize_github_url("https://npmjs.com/package/x"), "");
        assert_eq!(normalize_github_url(""), "");
    }

    #[test]
    fn parse_cim_date_wmi_datetime() {
        assert_eq!(
            parse_cim_date("20260820093405.123456+480").as_deref(),
            Some("2026-08-20 09:34:05")
        );
        assert_eq!(parse_cim_date("not a date"), None);
        assert_eq!(parse_cim_date(""), None);
    }

    #[test]
    fn release_body_strips_markup_and_keeps_chinese() {
        let body = "### v0.1.0-rc.8\n\n- 修复了 bug\n- 新增功能\n\n<h3 id=\"en\">English section</h3>\n- English item";
        let out = release_body_to_text(body);
        assert!(out.contains("修复了 bug"));
        assert!(out.contains("新增功能"));
        assert!(!out.contains("English item"));
        assert!(!out.contains("###"));
    }

    #[test]
    fn release_body_decodes_entities_and_links() {
        let out = release_body_to_text("see [docs](https://x/y) &amp; `code`");
        assert!(out.contains("see docs & code"), "实际输出: {out}");
    }

    #[test]
    fn extract_analysis_json_picks_last_valid() {
        let text = "some text\n{\"score\":3,\"verdict\":\"一般\"}\nmore\n{\"score\":8,\"verdict\":\"真实有用\"}";
        let v = extract_analysis_json(text).expect("应提取到 JSON");
        assert_eq!(v["score"], 8);
        assert_eq!(v["verdict"], "真实有用");
    }

    #[test]
    fn extract_analysis_json_ignores_invalid() {
        assert!(extract_analysis_json("no json here").is_none());
        // 不含 score / verdict 的对象不算评分卡。
        assert!(extract_analysis_json("{\"foo\":1}").is_none());
        // 只要带 score/verdict 就接受（哪怕 score 是字符串）。
        let v = extract_analysis_json("{\"score\":\"x\",\"verdict\":\"真实有用\"}").unwrap();
        assert_eq!(v["score"], "x");
    }

    #[test]
    fn split_log_chunk_trims_and_drops_blanks() {
        assert_eq!(
            split_log_chunk("added 12 packages\n"),
            vec!["added 12 packages"]
        );
        assert_eq!(split_log_chunk("a\r\nb\r\n"), vec!["a", "b"]);
        assert_eq!(split_log_chunk("a\n\n\nb\n"), vec!["a", "b"]);
        assert_eq!(
            split_log_chunk("trailing spaces   \n"),
            vec!["trailing spaces"]
        );
        assert!(split_log_chunk("").is_empty());
    }

    /// 裸令牌（不带 `key=` 前缀）也得打掉。
    ///
    /// `SECRET_KEYS` 那套只认 `token=xxx` 形式，一条单独出现的 `ghp_...`
    /// 会整条落进日志。令牌本来就不该走到这儿（见 token.rs 铁律 2），这是兜底。
    #[test]
    fn redact_secrets_masks_bare_github_tokens() {
        assert_eq!(
            redact_secrets("clone 失败: ghp_0123456789abcdefghijABCDEF"),
            "clone 失败: ghp_***"
        );
        assert_eq!(
            redact_secrets("github_pat_11ABCDEFG0abcdefghij"),
            "github_pat_***"
        );
        // 五种前缀都认。
        for p in ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"] {
            let line = format!("{p}0123456789abcdefghij");
            assert_eq!(redact_secrets(&line), format!("{p}***"), "漏了 {p}");
        }
        // 别误伤长得像的普通词。
        assert_eq!(redact_secrets("ghost_town"), "ghost_town");
        assert_eq!(redact_secrets("ghs_short"), "ghs_short");
        assert_eq!(
            redact_secrets("要填 ghp_ 开头的令牌"),
            "要填 ghp_ 开头的令牌"
        );
    }

    #[test]
    fn redact_secrets_masks_token_and_keys() {
        assert_eq!(
            redact_secrets("dsh web: http://127.0.0.1:3080/?token=D6Ro5ABDCTv_K--yk08q"),
            "dsh web: http://127.0.0.1:3080/?token=***"
        );
        assert_eq!(
            redact_secrets("http://h/?token=x&other=keep"),
            "http://h/?token=***&other=keep"
        );
        assert_eq!(
            redact_secrets("DEEPSEEK_API_KEY=abc123xyz789"),
            "DEEPSEEK_API_KEY=***"
        );
        assert_eq!(
            redact_secrets(r#"{"token": "abcdef", "score": 7}"#),
            r#"{"token": "***", "score": 7}"#
        );
        assert_eq!(
            redact_secrets("Authorization: Bearer eyJhbGci.abc"),
            "Authorization: Bearer ***"
        );
        assert_eq!(redact_secrets("key is sk-abc123def456"), "key is sk-***");
    }

    #[test]
    fn redact_secrets_leaves_normal_lines_alone() {
        // 用量页的中文文案里有 tokens 字样，不能被打码规则吃掉。
        for line in [
            "总会话数 12，输出 tokens: 34567，缓存读取 tokens 890",
            "dsh web: opening the default browser; pass --no-open to disable",
            "已是最新版本（0.1.5-rc.2）。",
            "[whale-balance] HTTP 余额接口请求失败: HTTP 401",
        ] {
            assert_eq!(redact_secrets(line), line, "不该改动: {line}");
        }
    }
}
