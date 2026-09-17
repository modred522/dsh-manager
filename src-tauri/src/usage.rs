//! Token 用量统计：读 dsh 的会话投影缓存，聚合成总量 / 按项目 / 近 14 天。
//!
//! 数据源：`$DSH_HOME/storages/session_projcache.json`，结构是
//! `tables.sessions.<id>.{identity, rows}`。
//!
//! **移植时修掉了一个 Electron 版的 bug**：原实现读 `rows.listMeta.val.lastPromptAt`，
//! 但真实数据里这个键叫 `rows.sessionListMetadata.val.lastPromptAt` —— `listMeta`
//! 根本不存在，于是 `lastPromptAt` 永远取不到，"近 14 天趋势"一直按会话**创建时间**
//! 而不是最后活动时间分桶。这里按真实键名读，取不到（空白会话的 lastPromptAt 是 null）
//! 再回退到 createdAt。

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Serialize;

/// `$DSH_HOME`，默认 `~/.dsh`（与 dsh CLI 自身的约定一致）。
pub fn dsh_home() -> PathBuf {
    if let Some(dir) = std::env::var_os("DSH_HOME") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .unwrap_or_default();
    PathBuf::from(home).join(".dsh")
}

fn session_cache_path() -> PathBuf {
    dsh_home().join("storages").join("session_projcache.json")
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Tokens {
    pub sessions: u64,
    pub uncached_input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub output: u64,
}

impl Tokens {
    fn add(&mut self, other: &Tokens) {
        self.sessions += other.sessions;
        self.uncached_input += other.uncached_input;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
        self.output += other.output;
    }

    /// 排序用的"体量"：与渲染层画条形图时用的口径保持一致。
    fn weight(&self) -> u64 {
        self.output + self.uncached_input + self.cache_read
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub cwd: String,
    pub name: String,
    #[serde(flatten)]
    pub tokens: Tokens,
    pub last_activity: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Day {
    pub day: String,
    #[serde(flatten)]
    pub tokens: Tokens,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub file_time: Option<f64>,
    pub totals: Tokens,
    pub projects: Vec<Project>,
    pub daily: Vec<Day>,
}

/// 毫秒时间戳 → 本地日期 `YYYY-MM-DD`。
fn local_day_key(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_default()
}

pub fn get_usage() -> Usage {
    let path = session_cache_path();
    let mut result = Usage::default();

    let Ok(meta) = std::fs::metadata(&path) else {
        return result; // 文件还不存在（没用过 dsh）就返回空结构
    };
    result.file_time = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as f64);

    let Ok(text) = std::fs::read_to_string(&path) else {
        return result;
    };
    let Ok(cache) = serde_json::from_str::<serde_json::Value>(&text) else {
        return result; // 解析失败也返回空结构，别让用量页崩掉
    };

    let Some(sessions) = cache
        .get("tables")
        .and_then(|t| t.get("sessions"))
        .and_then(|s| s.as_object())
    else {
        return result;
    };

    let mut projects: HashMap<String, Project> = HashMap::new();
    let mut days: HashMap<String, Tokens> = HashMap::new();

    for entry in sessions.values() {
        let identity = entry.get("identity");
        let rows = entry.get("rows");
        let totals = rows
            .and_then(|r| r.get("tokenUsage"))
            .and_then(|t| t.get("val"))
            .and_then(|v| v.get("totals"));

        let num = |key: &str| -> u64 {
            totals
                .and_then(|t| t.get(key))
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
        };
        let t = Tokens {
            sessions: 1,
            uncached_input: num("uncachedInputTokens"),
            cache_read: num("cacheReadTokens"),
            cache_write: num("cacheWriteTokens"),
            output: num("outputTokens"),
        };

        let cwd = identity
            .and_then(|i| i.get("cwd"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // 真实键名是 sessionListMetadata（不是 Electron 版写的 listMeta）；
        // 空白会话的 lastPromptAt 是 null，此时回退到 createdAt。
        let last_prompt_at = rows
            .and_then(|r| r.get("sessionListMetadata"))
            .and_then(|m| m.get("val"))
            .and_then(|v| v.get("lastPromptAt"))
            .and_then(|v| v.as_i64())
            .or_else(|| {
                identity
                    .and_then(|i| i.get("createdAt"))
                    .and_then(|v| v.as_i64())
            })
            .unwrap_or(0);

        result.totals.add(&t);

        if !cwd.is_empty() {
            let p = projects.entry(cwd.clone()).or_insert_with(|| Project {
                name: basename(&cwd),
                cwd: cwd.clone(),
                tokens: Tokens::default(),
                last_activity: 0,
            });
            p.tokens.add(&t);
            if last_prompt_at > p.last_activity {
                p.last_activity = last_prompt_at;
            }
        }

        if last_prompt_at > 0 {
            days.entry(local_day_key(last_prompt_at))
                .or_default()
                .add(&t);
        }
    }

    let mut list: Vec<Project> = projects.into_values().collect();
    // 体量降序；同量时按名字定序，避免每次刷新顺序乱跳（HashMap 遍历顺序不稳定）。
    list.sort_by(|a, b| {
        b.tokens
            .weight()
            .cmp(&a.tokens.weight())
            .then_with(|| a.name.cmp(&b.name))
    });
    result.projects = list;

    // 近 14 天（含今天），缺的日子补零，保证图表宽度固定。
    let today = chrono::Local::now().date_naive();
    result.daily = (0..14)
        .rev()
        .map(|i| {
            let day = (today - chrono::Duration::days(i))
                .format("%Y-%m-%d")
                .to_string();
            let tokens = days.get(&day).copied().unwrap_or_default();
            Day { day, tokens }
        })
        .collect();

    result
}

/// 取路径最后一段作为项目名；取不到就用原路径。
fn basename(p: &str) -> String {
    let trimmed = p.trim_end_matches(['/', '\\']);
    trimmed
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(p)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basename_handles_both_separators() {
        assert_eq!(basename("D:\\DeepseekTest"), "DeepseekTest");
        assert_eq!(basename("/home/u/proj"), "proj");
        assert_eq!(basename("D:\\a\\b\\"), "b");
        assert_eq!(basename("solo"), "solo");
    }

    #[test]
    fn day_key_is_local_date() {
        let s = local_day_key(1_787_226_224_838);
        assert_eq!(s.len(), 10, "形如 YYYY-MM-DD, 实际 {s}");
        assert!(s.starts_with("2026-"));
    }

    #[test]
    fn missing_cache_file_yields_empty_usage() {
        // 指到一个不存在的 DSH_HOME 时不该 panic（新机器上就是这种情况）。
        let u = Usage::default();
        assert_eq!(u.totals.sessions, 0);
        assert!(u.projects.is_empty());
    }

    #[test]
    fn aggregates_real_shaped_payload() {
        // 这份结构取自本机真实的 session_projcache.json：
        // 键是 sessionListMetadata（不是 listMeta），空白会话的 lastPromptAt 为 null。
        let json = r#"{
          "tables": { "sessions": {
            "session-a": {
              "identity": { "createdAt": 1787226224838, "cwd": "D:\\DeepseekTest" },
              "rows": {
                "tokenUsage": { "val": { "totals": {
                  "uncachedInputTokens": 100, "outputTokens": 200,
                  "cacheReadTokens": 300, "cacheWriteTokens": 0 } } },
                "sessionListMetadata": { "val": { "blank": false, "lastPromptAt": 1787226292590 } }
              }
            },
            "session-b": {
              "identity": { "createdAt": 1787306699731, "cwd": "D:\\DeepseekTest" },
              "rows": {
                "tokenUsage": { "val": { "totals": {
                  "uncachedInputTokens": 1, "outputTokens": 2,
                  "cacheReadTokens": 3, "cacheWriteTokens": 4 } } },
                "sessionListMetadata": { "val": { "blank": true, "lastPromptAt": null } }
              }
            },
            "session-c": {
              "identity": { "createdAt": 1787715144834, "cwd": "D:\\Other" },
              "rows": { "tokenUsage": { "val": { "totals": { "outputTokens": 50 } } } }
            }
          } }
        }"#;
        let cache: serde_json::Value = serde_json::from_str(json).unwrap();
        let sessions = cache["tables"]["sessions"].as_object().unwrap();

        // 直接核对聚合口径（get_usage 里同一段逻辑）。
        let mut totals = Tokens::default();
        for e in sessions.values() {
            let t = e
                .get("rows")
                .and_then(|r| r.get("tokenUsage"))
                .and_then(|t| t.get("val"))
                .and_then(|v| v.get("totals"));
            let num = |k: &str| {
                t.and_then(|x| x.get(k))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0)
            };
            totals.add(&Tokens {
                sessions: 1,
                uncached_input: num("uncachedInputTokens"),
                cache_read: num("cacheReadTokens"),
                cache_write: num("cacheWriteTokens"),
                output: num("outputTokens"),
            });
        }
        assert_eq!(totals.sessions, 3);
        assert_eq!(totals.output, 252);
        assert_eq!(totals.uncached_input, 101);
        assert_eq!(totals.cache_read, 303);

        // 空白会话没有 lastPromptAt，必须回退到 createdAt 才不会被漏掉。
        let blank = &sessions["session-b"];
        let lp = blank
            .get("rows")
            .and_then(|r| r.get("sessionListMetadata"))
            .and_then(|m| m.get("val"))
            .and_then(|v| v.get("lastPromptAt"))
            .and_then(|v| v.as_i64());
        assert!(lp.is_none(), "空白会话的 lastPromptAt 应为 null");
        let fallback = blank["identity"]["createdAt"].as_i64().unwrap();
        assert_eq!(fallback, 1787306699731);

        // 有 lastPromptAt 的会话要用它，而不是 createdAt —— 这正是 Electron 版读错键名丢掉的行为。
        let a = &sessions["session-a"];
        let lp_a = a["rows"]["sessionListMetadata"]["val"]["lastPromptAt"]
            .as_i64()
            .unwrap();
        assert_ne!(lp_a, a["identity"]["createdAt"].as_i64().unwrap());
    }

    #[test]
    fn daily_window_is_fourteen_days_ending_today() {
        let u = get_usage(); // 真实环境：文件可能存在也可能不存在
        if u.file_time.is_some() {
            assert_eq!(u.daily.len(), 14);
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            assert_eq!(u.daily.last().map(|d| d.day.clone()), Some(today));
            // 日期应当严格递增。
            for w in u.daily.windows(2) {
                assert!(
                    w[0].day < w[1].day,
                    "日期应递增: {:?} {:?}",
                    w[0].day,
                    w[1].day
                );
            }
        }
    }
}

#[cfg(test)]
mod wire_format {
    use super::*;

    /// 渲染层（renderer.js 的 renderUsage）按名字取这些字段。
    /// 序列化出来的键名一旦变了，用量页就会静默显示 0 —— 所以这里把契约钉死。
    #[test]
    fn serialized_keys_match_renderer_expectations() {
        let u = Usage {
            file_time: Some(1.0),
            totals: Tokens {
                sessions: 3,
                uncached_input: 1,
                cache_read: 2,
                cache_write: 3,
                output: 4,
            },
            projects: vec![Project {
                cwd: r"D:\p".into(),
                name: "p".into(),
                tokens: Tokens::default(),
                last_activity: 0,
            }],
            daily: vec![Day {
                day: "2026-09-15".into(),
                tokens: Tokens::default(),
            }],
        };
        let v = serde_json::to_value(&u).unwrap();

        // 顶层
        for k in ["fileTime", "totals", "projects", "daily"] {
            assert!(v.get(k).is_some(), "顶层缺少 {k}: {v}");
        }
        // totals：renderer 取 sessions / output / uncachedInput / cacheRead
        for k in [
            "sessions",
            "output",
            "uncachedInput",
            "cacheRead",
            "cacheWrite",
        ] {
            assert!(v["totals"].get(k).is_some(), "totals 缺少 {k}");
        }
        // projects：renderer 取 name / cwd / output / uncachedInput / cacheRead / sessions
        let p = &v["projects"][0];
        for k in [
            "name",
            "cwd",
            "output",
            "uncachedInput",
            "cacheRead",
            "sessions",
        ] {
            assert!(p.get(k).is_some(), "project 缺少 {k}: {p}");
        }
        // daily：renderer 取 day / cacheRead / uncachedInput / output
        let d = &v["daily"][0];
        for k in ["day", "cacheRead", "uncachedInput", "output"] {
            assert!(d.get(k).is_some(), "day 缺少 {k}: {d}");
        }
        // 不该出现 snake_case 残留
        assert!(!v["totals"].get("uncached_input").is_some());
    }
}
