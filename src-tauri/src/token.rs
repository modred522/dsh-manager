//! 可选的 GitHub 令牌。
//!
//! 为什么需要：GitHub 未登录接口按**出口 IP** 限 60 次/小时。公司网络走 NAT 时
//! 这个额度是整个办公室共用的，插件市场的详情页和分析经常直接打不开
//! （docs 第十四节记了实测：core 配额 0/60）。带上令牌是 5000 次/小时。
//!
//! **不存明文**：令牌写进 Windows 凭据管理器（`CredWriteW`，generic 类型，
//! 只在本机持久化），不进 `config.json` —— 配置文件是明文，任何以当前用户身份
//! 运行的进程都读得到。凭据管理器至少做到随用户凭据加密，而且用户能在
//! "控制面板 → 凭据管理器 → Windows 凭据"里自己看到并删掉它。
//!
//! 也认 `GITHUB_TOKEN` / `GH_TOKEN` 环境变量，且优先级更高：CI 和习惯用 `gh`
//! 的人不用再单独存一份。
//!
//! 三条铁律，改这个文件前先读：
//!   1. **完整令牌绝不回传渲染层**，只给尾 4 位（`Status::hint`）。
//!   2. **完整令牌绝不进日志**。`pure::redact_secrets` 另外加了 PAT 前缀兜底。
//!   3. **只往 `api.github.com` 发**，见 `market::token_for`。塞进
//!      `default_headers` 就会跟着 npm registry 的请求一起发出去。

use std::sync::Mutex;

use serde::Serialize;

/// 凭据管理器里的条目名。用户会在凭据管理器界面里看到这一行，所以写得像人话。
const TARGET: &str = "dsh-manager:github-token";

/// 环境变量按这个顺序找。`GH_TOKEN` 是 `gh` CLI 的习惯名。
const ENV_KEYS: [&str; 2] = ["GITHUB_TOKEN", "GH_TOKEN"];

/// 令牌来源。渲染层据此决定设置项显示"已保存"还是"由环境变量提供（只读）"。
#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// `none` | `env` | `store`
    pub source: String,
    /// 尾 4 位，用来确认"存的是不是我以为的那个"。**不是完整令牌。**
    pub hint: String,
    /// `source == "env"` 时是变量名，让用户知道该去改哪儿。
    pub env_key: String,
}

/// 只给出尾 4 位。长度不够就整条打掉 —— 短令牌本来就不合法，
/// 露出"尾 4 位"可能等于露出大半条。
pub fn hint(token: &str) -> String {
    let n = token.chars().count();
    if n <= 8 {
        return "*".repeat(n.min(8));
    }
    format!("…{}", token.chars().skip(n - 4).collect::<String>())
}

/// 存之前先看形状。
///
/// 拦掉的是"粘贴时带进了换行/空格"这类事故：那种值塞进 HTTP 头会变成一条
/// 莫名其妙的错误，比直接说"格式不对"难查得多。不校验前缀 ——
/// classic（`ghp_`）、fine-grained（`github_pat_`）、OAuth（`gho_`）、
/// 将来的新格式，白名单只会挡住合法令牌。
pub fn validate(token: &str) -> Result<(), String> {
    if token.is_empty() {
        return Err("令牌是空的".into());
    }
    if token.chars().any(|c| c.is_whitespace()) {
        return Err("令牌里有空格或换行（粘贴时常见），请检查后重试".into());
    }
    if !token.chars().all(|c| c.is_ascii_graphic()) {
        return Err("令牌含非 ASCII 字符，不像是 GitHub 令牌".into());
    }
    if token.chars().count() < 20 {
        return Err("令牌太短，不像是 GitHub 令牌".into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 读取
// ---------------------------------------------------------------------------

fn from_env() -> Option<(&'static str, String)> {
    for key in ENV_KEYS {
        if let Ok(v) = std::env::var(key) {
            let v = v.trim().to_string();
            if !v.is_empty() {
                return Some((key, v));
            }
        }
    }
    None
}

/// 凭据管理器的读取结果缓存。
///
/// 外层 `Option` 是"查过没有"，内层是"有没有值"。市场搜索一批要发 4 个请求，
/// 每个都去敲一次凭据存储没必要，某些终端防护软件还会对频繁读凭据报警。
/// `save` / `clear` 负责失效。
static CACHE: Mutex<Option<Option<String>>> = Mutex::new(None);

fn invalidate() {
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

fn cached_store() -> Option<String> {
    let mut g = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    g.get_or_insert_with(|| read_store(TARGET)).clone()
}

/// 当前生效的令牌。环境变量优先。
pub fn load() -> Option<String> {
    if let Some((_, v)) = from_env() {
        return Some(v);
    }
    cached_store()
}

pub fn status() -> Status {
    if let Some((key, v)) = from_env() {
        return Status {
            source: "env".into(),
            hint: hint(&v),
            env_key: key.into(),
        };
    }
    match cached_store() {
        Some(v) => Status {
            source: "store".into(),
            hint: hint(&v),
            env_key: String::new(),
        },
        None => Status {
            source: "none".into(),
            ..Default::default()
        },
    }
}

// ---------------------------------------------------------------------------
// 写入 / 删除（Windows 凭据管理器）
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn read_store(target_name: &str) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };

    let target = wide(target_name);
    let mut ptr: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: target 是以 0 结尾的 UTF-16 缓冲，整个调用期间都活着。
    // ptr 由 CredReadW 分配，取完值立刻 CredFree，不外传。
    unsafe {
        if CredReadW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None, &mut ptr).is_err() {
            return None; // 没存过就是 ERROR_NOT_FOUND，属正常
        }
        if ptr.is_null() {
            return None;
        }
        let cred = &*ptr;
        let out = if cred.CredentialBlob.is_null() || cred.CredentialBlobSize == 0 {
            None
        } else {
            let bytes =
                std::slice::from_raw_parts(cred.CredentialBlob, cred.CredentialBlobSize as usize);
            String::from_utf8(bytes.to_vec()).ok()
        };
        CredFree(ptr as *const _);
        out
    }
}

#[cfg(windows)]
fn write_store(target_name: &str, token: &str) -> Result<(), String> {
    use windows::core::PWSTR;
    use windows::Win32::Security::Credentials::{
        CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    let mut target = wide(target_name);
    let mut user = wide("github");
    let mut blob = token.as_bytes().to_vec();
    let cred = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: PWSTR(user.as_mut_ptr()),
        ..Default::default()
    };
    // SAFETY: cred 里三个指针都指向本函数栈上的缓冲，它们活到函数返回，
    // 晚于 CredWriteW 返回。CredWriteW 只读不写这些缓冲。
    unsafe { CredWriteW(&cred, 0) }.map_err(|e| format!("写入凭据管理器失败: {e}"))
}

#[cfg(windows)]
fn delete_store(target_name: &str) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC};

    let target = wide(target_name);
    // SAFETY: target 是以 0 结尾的 UTF-16 缓冲，调用期间一直活着。
    let r = unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None) };
    match r {
        Ok(()) => Ok(()),
        // 本来就没存过也算清干净了 —— 清除要幂等。
        Err(_) if read_store(target_name).is_none() => Ok(()),
        Err(e) => Err(format!("删除凭据失败: {e}")),
    }
}

#[cfg(not(windows))]
fn read_store(_target_name: &str) -> Option<String> {
    None
}

#[cfg(not(windows))]
fn write_store(_target_name: &str, _token: &str) -> Result<(), String> {
    Err("只有 Windows 版支持保存令牌，其他平台请用 GITHUB_TOKEN 环境变量".into())
}

#[cfg(not(windows))]
fn delete_store(_target_name: &str) -> Result<(), String> {
    Ok(())
}

pub fn save(token: &str) -> Result<(), String> {
    validate(token)?;
    let r = write_store(TARGET, token);
    invalidate();
    r
}

pub fn clear() -> Result<(), String> {
    let r = delete_store(TARGET);
    invalidate();
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hint_never_reveals_a_usable_prefix() {
        assert_eq!(hint("ghp_abcdefghijklmnop"), "…mnop");
        // 太短的整条打掉：只露 4 位对短串等于露了大半。
        assert_eq!(hint("abcdefgh"), "********");
        assert_eq!(hint("abc"), "***");
        assert_eq!(hint(""), "");
        // 不管怎样都不能把完整令牌带出来。
        let t = "ghp_0123456789abcdefghij";
        assert!(!hint(t).contains("0123456789"));
    }

    #[test]
    fn validate_rejects_paste_accidents() {
        let good = "ghp_0123456789abcdefghij";
        assert!(validate(good).is_ok());
        // 换行/空格是粘贴时最常见的事故，塞进 HTTP 头只会换回一条看不懂的错。
        assert!(validate(&format!("{good}\n")).is_err());
        assert!(validate("ghp_0123 456789abcdefghij").is_err());
        assert!(validate("").is_err());
        assert!(validate("ghp_short").is_err());
        assert!(validate("令牌令牌令牌令牌令牌令牌令牌令牌令牌令牌").is_err());
        // 不做前缀白名单：fine-grained 和将来的新格式都得放过去。
        assert!(validate("github_pat_0123456789abcdefghij").is_ok());
    }

    /// 凭据管理器的读写删往返。这段是 unsafe FFI，必须真跑一遍才算验过。
    ///
    /// 用一次性的条目名，不碰用户真正保存的那条；用完删掉，不留痕。
    #[cfg(windows)]
    #[test]
    fn credential_store_round_trip() {
        const T: &str = "dsh-manager:test-github-token-roundtrip";
        let secret = "ghp_testtoken0123456789abcdef";

        // 先清一遍，保证上次失败留下的残留不影响判断。
        let _ = delete_store(T);
        assert_eq!(read_store(T), None, "清过之后应该读不到");

        write_store(T, secret).expect("写入凭据管理器失败");
        assert_eq!(
            read_store(T).as_deref(),
            Some(secret),
            "读回来的值和写进去的不一致"
        );

        // 覆盖写要生效，而不是追加出第二条。
        let second = "ghp_secondvalue0123456789abcd";
        write_store(T, second).expect("覆盖写失败");
        assert_eq!(read_store(T).as_deref(), Some(second));

        delete_store(T).expect("删除失败");
        assert_eq!(read_store(T), None, "删完还能读到");
        // 删除要幂等：本来就没有也算成功。
        delete_store(T).expect("重复删除应当是幂等的");
    }

    /// `status()` 是回传渲染层的东西，序列化结果里绝不能出现完整令牌。
    #[test]
    fn status_serializes_only_a_masked_hint() {
        let s = Status {
            source: "store".into(),
            hint: hint("ghp_0123456789abcdefghij"),
            env_key: String::new(),
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v.get("source").and_then(|x| x.as_str()), Some("store"));
        assert!(v.get("envKey").is_some(), "应为 camelCase: {v}");
        let text = v.to_string();
        assert!(!text.contains("0123456789"), "完整令牌泄漏进状态: {text}");
        // 结构里只有这三个字段，别哪天顺手加个 token 进去。
        assert_eq!(v.as_object().unwrap().len(), 3, "字段数变了: {v}");
    }
}
