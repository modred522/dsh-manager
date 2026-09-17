//! DSH 管理器 — Tauri 版主模块。
//!
//! 已完成：状态/启停/托盘/日志（阶段一）、更新回滚/通知/快捷键/自启/快捷方式（阶段二）、
//! 用量统计/插件管理（阶段三）、插件市场/分析管线（阶段四）。29 个命令全部接通。
//!
//! 与 Electron 版的对应关系见 `docs/TAURI-MIGRATION.md`。整个迁移刻意保持
//! **渲染层零改动**：`renderer/tauri-bridge.js` 把 `window.dsh.*` 映射到
//! Tauri 的 `invoke`/`listen`，所以 `renderer.js` / `market.js` 不用动。

pub mod analysis;
pub mod config;
pub mod dsh;
pub mod gh;
pub mod logging;
pub mod market;
pub mod plugins;
pub mod procs;
pub mod pure;
pub mod shortcut;
pub mod texts;
pub mod token;
pub mod updates;
pub mod usage;

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use config::Config;

pub struct AppState {
    pub config: Mutex<Config>,
    pub npm_prefix: Mutex<String>,
    pub latest_version: Mutex<Option<String>>,
    pub last_check_time: Mutex<Option<String>>,
    pub busy: Mutex<Option<String>>,
    /// dsh CLI 自身的依赖集合：市场搜索要把这些核心包排除掉。
    pub core_packages: Mutex<std::collections::HashSet<String>>,
}

impl AppState {
    fn new(cfg: Config) -> Self {
        Self {
            config: Mutex::new(cfg),
            npm_prefix: Mutex::new(String::new()),
            latest_version: Mutex::new(None),
            last_check_time: Mutex::new(None),
            busy: Mutex::new(None),
            core_packages: Mutex::new(std::collections::HashSet::new()),
        }
    }

    fn cfg(&self) -> Config {
        self.config
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn prefix(&self) -> String {
        self.npm_prefix
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

#[derive(Serialize)]
struct RuntimeItem {
    name: String,
    version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    app_version: String,
    runtime: Vec<RuntimeItem>,
}

/// 与 Electron 版 `sendState()` 的载荷逐字段对齐，渲染层因此不用改。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    installed_version: Option<String>,
    latest_version: Option<String>,
    dsh_url: String,
    server_up: bool,
    dsh_running: bool,
    dsh_processes: Vec<procs::DshProcess>,
    busy: Option<String>,
    last_check_time: Option<String>,
    rollback_version: Option<String>,
    config: Config,
    app_info: AppInfo,
}

fn app_info(app: &AppHandle) -> AppInfo {
    AppInfo {
        app_version: app.package_info().version.to_string(),
        // 名称由后端给出，渲染层照着渲染（Electron 版给的是 Electron/Node.js/Chromium）。
        runtime: vec![
            RuntimeItem {
                name: "Tauri".into(),
                version: tauri::VERSION.to_string(),
            },
            RuntimeItem {
                name: "Rust".into(),
                version: env!("CARGO_PKG_RUST_VERSION").to_string(),
            },
            RuntimeItem {
                name: "WebView2".into(),
                version: tauri::webview_version().unwrap_or_else(|_| "—".into()),
            },
        ],
    }
}

async fn build_state(app: &AppHandle) -> State {
    let st = app.state::<AppState>();
    let cfg = st.cfg();
    let prefix = st.prefix();
    let port = config::dsh_port(&cfg);

    let server_up = dsh::is_server_up(&cfg.dsh_url, Duration::from_millis(1500)).await;
    let processes = procs::dsh_processes(port);

    update_tray(app, processes.len());

    // 先把锁里的值取到局部变量：结构体字面量作为尾表达式时，
    // 里面的 MutexGuard 临时值会活到 `st` 之后，直接内联会借用期报错。
    let latest_version = st
        .latest_version
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let busy = st.busy.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let last_check_time = st
        .last_check_time
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();

    State {
        installed_version: dsh::installed_version(&prefix),
        latest_version,
        dsh_url: cfg.dsh_url.clone(),
        server_up,
        dsh_running: dsh::is_dsh_running(),
        dsh_processes: processes,
        busy,
        last_check_time,
        rollback_version: cfg.rollback_version.clone(),
        config: cfg,
        app_info: app_info(app),
    }
}

/// 推送状态给所有窗口。`app.emit` 天然是全窗口广播，
/// 取代了 Electron 版手写的 `broadcast()`（主窗口 + 市场窗口两个句柄）。
async fn send_state(app: &AppHandle) {
    let s = build_state(app).await;
    let _ = app.emit("state", &s);
}

// ---------------------------------------------------------------------------
// 命令
// ---------------------------------------------------------------------------

#[tauri::command]
async fn get_state(app: AppHandle) -> State {
    build_state(&app).await
}

#[tauri::command]
async fn open_dsh(app: AppHandle) -> Result<(), String> {
    let cfg = app.state::<AppState>().cfg();
    let url = cfg.dsh_url.clone();

    if dsh::is_server_up(&url, Duration::from_millis(1500)).await {
        logging::log("DSH 服务已在运行，直接打开浏览器。");
        open_url(&app, &url);
        send_state(&app).await;
        return Ok(());
    }

    logging::log("启动 DSH（dsh web）...");
    if let Err(e) = dsh::launch_dsh() {
        logging::log(format!("启动失败: {e}"));
        send_state(&app).await;
        return Err(e.to_string());
    }

    // 最多等 30 秒，每秒探一次（与 Electron 版一致）。
    for _ in 0..30 {
        if !dsh::is_dsh_running() {
            logging::log("DSH 进程已退出，启动可能失败（见日志）。");
            send_state(&app).await;
            return Ok(());
        }
        if dsh::is_server_up(&url, Duration::from_millis(1000)).await {
            logging::log("DSH 服务已就绪，打开浏览器。");
            open_url(&app, &url);
            send_state(&app).await;
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    logging::log("等待 DSH 就绪超时（30 秒），请查看日志确认。");
    send_state(&app).await;
    Ok(())
}

#[tauri::command]
async fn stop_dsh(app: AppHandle, pids: Option<Vec<u32>>) -> usize {
    let cfg = app.state::<AppState>().cfg();
    let n = procs::stop_processes(config::dsh_port(&cfg), pids.as_deref());
    // taskkill 是异步生效的，给端口释放留点时间再报数。
    tokio::time::sleep(Duration::from_millis(800)).await;
    dsh::clear_child(true);
    if n > 0 {
        logging::log(format!("已停止 {n} 个 DSH 进程。"));
    }
    send_state(&app).await;
    n
}

#[tauri::command]
async fn restart_dsh(app: AppHandle) -> Result<(), String> {
    logging::log("重启 DSH...");
    let cfg = app.state::<AppState>().cfg();
    let n = procs::stop_processes(config::dsh_port(&cfg), None);
    if n > 0 {
        logging::log(format!("已停止 {n} 个 DSH 进程。"));
    }
    dsh::clear_child(true);
    tokio::time::sleep(Duration::from_millis(1200)).await;
    open_dsh(app).await
}

#[tauri::command]
fn get_recent_logs() -> Vec<String> {
    logging::recent_log_lines()
}

/// 渲染层自身的错误回传到后端日志。
///
/// 存在的理由：桥接层失败（比如 capability 没给 `core:event:default` 导致 `listen()`
/// 被拒）原先只会显示在界面的日志区，**落不到日志文件**，于是启动烟测、CI 一概看不见——
/// 前端事件通道整条死掉，自动化检查却全绿。现在这类错误会进日志文件，
/// 烟测就能把它当失败信号。
///
/// 注意这个命令走 `invoke`（自定义命令不受 ACL 管），所以哪怕事件权限坏了它照样能用。
#[tauri::command]
fn log_frontend_error(message: String) {
    let msg = message.trim();
    if msg.is_empty() {
        return;
    }
    // 截断一下，别让前端的长堆栈把日志文件撑爆。
    let shown: String = msg.chars().take(500).collect();
    logging::log(format!("[前端错误] {shown}"));
}

#[tauri::command]
async fn set_config(app: AppHandle, cfg: serde_json::Value) -> Result<(), String> {
    let st = app.state::<AppState>();
    // 渲染层只送部分字段过来，和现有配置合并后再落盘（对齐 Electron 的 {...config, ...next}）。
    let merged = {
        let current = st.cfg();
        let mut base = serde_json::to_value(&current).map_err(|e| e.to_string())?;
        if let (Some(b), Some(n)) = (base.as_object_mut(), cfg.as_object()) {
            for (k, v) in n {
                b.insert(k.clone(), v.clone());
            }
        }
        serde_json::from_value::<Config>(base).map_err(|e| e.to_string())?
    };

    let (theme_changed, autostart_changed, interval_changed) = {
        let mut guard = st.config.lock().unwrap_or_else(|e| e.into_inner());
        let changes = (
            guard.theme != merged.theme,
            guard.auto_start_with_windows != merged.auto_start_with_windows,
            guard.auto_check_on_startup != merged.auto_check_on_startup
                || guard.auto_check_interval_hours != merged.auto_check_interval_hours,
        );
        *guard = merged.clone();
        changes
    };
    config::save(&merged).map_err(|e| e.to_string())?;
    if theme_changed {
        apply_theme(&app, &merged.theme);
    }
    if autostart_changed {
        apply_autostart(&app, merged.auto_start_with_windows);
    }
    if interval_changed {
        // 间隔变了要重新起定时器，否则改了设置得重启才生效。
        restart_auto_check_timer(&app, &merged);
    }
    send_state(&app).await;
    Ok(())
}

#[tauri::command]
fn open_config_dir(app: AppHandle) {
    open_path(&app, &config::config_dir().to_string_lossy());
}

#[tauri::command]
fn open_npm_dir(app: AppHandle) {
    let prefix = app.state::<AppState>().prefix();
    let target = if prefix.is_empty() {
        config::config_dir().to_string_lossy().into_owned()
    } else {
        prefix
    };
    open_path(&app, &target);
}

/// 只允许 http(s) 外链（渲染层点 GitHub/npm 链接时用）。
#[tauri::command]
fn open_external(app: AppHandle, url: String) -> bool {
    if url.starts_with("http://") || url.starts_with("https://") {
        open_url(&app, &url);
        return true;
    }
    false
}

/// 打开插件市场独立窗口。
///
/// **这个命令必须是 `async` 的，别改回同步。** Windows 上
/// `WebviewWindowBuilder::build()` 会把建窗任务投递给事件循环再阻塞等结果，
/// 而同步 `#[tauri::command]` 就跑在事件循环所在的主线程上 —— 于是互相等死。
/// 表现是：窗口出来了但永远白屏、关不掉、还比主窗口活得久。
/// `async` 命令跑在 async runtime 上，不占着事件循环，才能正常建窗。
#[tauri::command]
async fn open_market(app: AppHandle) -> Result<(), String> {
    // 已打开则聚焦，否则新建（对齐 Electron 版 createMarketWindow）。
    if let Some(win) = app.get_webview_window("market") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }
    let cfg = app.state::<AppState>().cfg();
    WebviewWindowBuilder::new(&app, "market", WebviewUrl::App("market.html".into()))
        .title(texts::t(&cfg.language, texts::Key::MarketTitle))
        .inner_size(920.0, 760.0)
        .min_inner_size(680.0, 560.0)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

// --- 更新 / 回滚 --------------------------------------------------------------

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    current: Option<String>,
    latest: Option<String>,
    has_update: bool,
    /// 预发布版必须标注：`all` 通道会把 next 上的 alpha 也算进来，
    /// 不写明的话用户点一下"更新"就被静默带上了 alpha。
    prerelease: bool,
    changelog: Option<String>,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InstallResult {
    ok: bool,
    new_version: Option<String>,
    reason: Option<String>,
}

#[tauri::command]
async fn check_updates(app: AppHandle, silent: Option<bool>) -> CheckResult {
    let silent = silent.unwrap_or(false);
    if busy_get(&app).is_some() {
        return CheckResult::default();
    }
    busy_set(&app, Some("check"));
    send_state(&app).await;

    let cfg = app.state::<AppState>().cfg();
    let prefix = app.state::<AppState>().prefix();
    let mut result = CheckResult {
        current: dsh::installed_version(&prefix),
        ..Default::default()
    };

    if !silent {
        logging::log("正在检查更新...");
    }
    let latest = updates::latest_dsh_version(&cfg.update_channel).await;
    {
        let st = app.state::<AppState>();
        *st.latest_version.lock().unwrap_or_else(|e| e.into_inner()) = latest.clone();
        *st.last_check_time.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(chrono::Local::now().to_rfc3339());
    }
    result.latest = latest.clone();

    match (result.current.as_deref(), latest.as_deref()) {
        (Some(current), Some(latest)) => {
            if pure::compare_versions(latest, current) == std::cmp::Ordering::Greater {
                result.has_update = true;
                result.prerelease = pure::is_prerelease(latest);
                logging::log(format!(
                    "发现新版本 {latest}{}（当前 {current}）。",
                    if result.prerelease {
                        "（预发布版）"
                    } else {
                        ""
                    }
                ));
                result.changelog = updates::fetch_changelog(latest).await;
                if silent {
                    notify(
                        &app,
                        &texts::t(&cfg.language, texts::Key::NotifyUpdateTitle),
                        &texts::notify_update_body(&cfg.language, latest, result.prerelease),
                    );
                }
            } else if !silent {
                logging::log(format!("已是最新版本（{current}）。"));
            }
        }
        _ => logging::log("检查更新失败：无法获取版本信息。"),
    }

    busy_set(&app, None);
    send_state(&app).await;
    result
}

/// 装一个指定版本（更新与回滚共用）：记运行态 → 停进程 → 任务栏进度 → 恢复。
async fn perform_install(app: &AppHandle, version: &str) -> (InstallResult, bool) {
    let cfg = app.state::<AppState>().cfg();
    let port = config::dsh_port(&cfg);
    let was_running = dsh::is_server_up(&cfg.dsh_url, Duration::from_millis(1500)).await
        || !procs::dsh_processes(port).is_empty();

    let stopped = procs::stop_processes(port, None);
    if stopped > 0 {
        logging::log(format!("安装前停止了 {stopped} 个 DSH 进程。"));
    }
    dsh::clear_child(true);

    busy_set(app, Some("update"));
    send_state(app).await;
    set_progress(app, Some(ProgressKind::Indeterminate));

    logging::log(format!("开始安装 @deepseek-ai/dsh@{version}..."));
    let code = updates::run_install(version).await;

    let prefix = app.state::<AppState>().prefix();
    let new_version = dsh::installed_version(&prefix);
    {
        let st = app.state::<AppState>();
        *st.latest_version.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    updates::clear_changelog_cache();

    set_progress(app, None);
    busy_set(app, None);
    send_state(app).await;

    (
        InstallResult {
            ok: code == 0,
            new_version,
            reason: None,
        },
        was_running,
    )
}

#[tauri::command]
async fn update(app: AppHandle) -> InstallResult {
    if busy_get(&app).is_some() {
        return InstallResult {
            reason: Some("busy".into()),
            ..Default::default()
        };
    }
    let prefix = app.state::<AppState>().prefix();
    let current = dsh::installed_version(&prefix);

    let mut latest = app
        .state::<AppState>()
        .latest_version
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    if latest.is_none() {
        latest = check_updates(app.clone(), Some(true)).await.latest;
    }

    let (Some(current), Some(latest)) = (current.as_deref(), latest.as_deref()) else {
        logging::log("当前已是最新版本，无需更新。");
        return InstallResult {
            reason: Some("uptodate".into()),
            ..Default::default()
        };
    };
    if pure::compare_versions(latest, current) != std::cmp::Ordering::Greater {
        logging::log("当前已是最新版本，无需更新。");
        return InstallResult {
            reason: Some("uptodate".into()),
            ..Default::default()
        };
    }

    let (r, was_running) = perform_install(&app, latest).await;
    let cfg = app.state::<AppState>().cfg();
    if r.ok {
        // npm 安装是覆盖式的、没有内置回滚，所以自己记下旧版本号。
        set_config_fields(&app, |c| c.rollback_version = Some(current.to_string()));
        let shown = r.new_version.clone().unwrap_or_else(|| "未知".into());
        logging::log(format!("更新完成，当前版本: {shown}"));
        notify(
            &app,
            &texts::t(&cfg.language, texts::Key::NotifyUpdatedTitle),
            &texts::notify_updated_body(&cfg.language, &shown),
        );
        if was_running {
            logging::log("之前 DSH 在运行，自动重新启动...");
            let _ = open_dsh(app.clone()).await;
        }
    } else {
        logging::log("更新结束，请查看上方日志。");
    }
    r
}

#[tauri::command]
async fn rollback(app: AppHandle) -> InstallResult {
    if busy_get(&app).is_some() {
        return InstallResult {
            reason: Some("busy".into()),
            ..Default::default()
        };
    }
    let cfg = app.state::<AppState>().cfg();
    let Some(target) = cfg.rollback_version.clone() else {
        return InstallResult {
            reason: Some("no-target".into()),
            ..Default::default()
        };
    };

    logging::log(format!("回滚到 {target}..."));
    let (r, was_running) = perform_install(&app, &target).await;
    if r.ok {
        set_config_fields(&app, |c| c.rollback_version = None);
        let shown = r.new_version.clone().unwrap_or_else(|| target.clone());
        logging::log(format!("回滚完成，当前版本: {shown}"));
        notify(
            &app,
            &texts::t(&cfg.language, texts::Key::NotifyRollbackTitle),
            &texts::notify_rollback_body(&cfg.language, &shown),
        );
        if was_running {
            logging::log("之前 DSH 在运行，自动重新启动...");
            let _ = open_dsh(app.clone()).await;
        }
    } else {
        logging::log("回滚失败，请查看上方日志。");
    }
    r
}

#[tauri::command]
async fn get_changelog(app: AppHandle, version: Option<String>) -> Option<String> {
    let v = match version {
        Some(v) if !v.is_empty() => v,
        _ => app
            .state::<AppState>()
            .latest_version
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()?,
    };
    updates::fetch_changelog(&v).await
}

// --- 以下为后续阶段；先占位，免得前端调用直接炸 -------------------------------

#[tauri::command]
fn get_usage() -> usage::Usage {
    usage::get_usage()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginList {
    plugins: Vec<plugins::Plugin>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginUpdateList {
    plugins: Vec<plugins::PluginUpdate>,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PluginResult {
    ok: bool,
    error: Option<String>,
}

#[tauri::command]
fn get_plugins() -> PluginList {
    PluginList {
        plugins: plugins::profile_plugins(),
    }
}

#[tauri::command]
async fn check_plugin_updates(app: AppHandle) -> PluginUpdateList {
    let cfg = app.state::<AppState>().cfg();
    PluginUpdateList {
        plugins: plugins::check_updates(&cfg.update_channel).await,
    }
}

/// 装 / 卸 / 升级三个动作只差一个命令和几句文案，共用同一条流程。
enum PluginAction {
    Install,
    Remove,
    Upgrade,
}

async fn run_plugin_action(
    app: &AppHandle,
    action: PluginAction,
    name: Option<String>,
) -> PluginResult {
    let pkg = name.unwrap_or_default().trim().to_string();
    if pkg.is_empty() {
        return PluginResult {
            ok: false,
            error: Some("包名不能为空".into()),
        };
    }
    if busy_get(app).is_some() {
        return PluginResult {
            ok: false,
            error: Some("有操作正在进行".into()),
        };
    }

    // 升级就是重新 add（dsh plugin add 会装到最新版本）。
    let (verb, sub) = match action {
        PluginAction::Install => ("安装", "add"),
        PluginAction::Remove => ("卸载", "remove"),
        PluginAction::Upgrade => ("升级", "add"),
    };

    busy_set(app, Some("plugin"));
    send_state(app).await;
    logging::log(format!(
        "{verb}插件 {pkg}（dsh plugin --profile web {sub} {pkg}）..."
    ));
    let code = plugins::run_plugin_command(&[sub, &pkg]).await;
    busy_set(app, None);
    send_state(app).await;

    if code == 0 {
        logging::log(format!("插件 {pkg} {verb}完成，重启 DSH 后生效。"));
        PluginResult {
            ok: true,
            error: None,
        }
    } else {
        logging::log(format!(
            "插件 {pkg} {verb}失败（退出码 {code}），见上方日志。"
        ));
        PluginResult {
            ok: false,
            error: Some(format!("退出码 {code}")),
        }
    }
}

#[tauri::command]
async fn install_plugin(app: AppHandle, name: Option<String>) -> PluginResult {
    run_plugin_action(&app, PluginAction::Install, name).await
}

#[tauri::command]
async fn remove_plugin(app: AppHandle, name: Option<String>) -> PluginResult {
    run_plugin_action(&app, PluginAction::Remove, name).await
}

#[tauri::command]
async fn upgrade_plugin(app: AppHandle, name: Option<String>) -> PluginResult {
    run_plugin_action(&app, PluginAction::Upgrade, name).await
}

#[tauri::command]
async fn market_search(
    app: AppHandle,
    source: Option<String>,
    query: Option<String>,
    reset: Option<bool>,
) -> market::SearchResult {
    let core = app
        .state::<AppState>()
        .core_packages
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    market::search_page(
        source.as_deref().unwrap_or("npm"),
        query.as_deref().unwrap_or(""),
        reset.unwrap_or(false),
        &core,
    )
    .await
}

#[tauri::command]
async fn plugin_info(name: Option<String>) -> Option<market::NpmInfo> {
    market::npm_plugin_info(name.unwrap_or_default().trim()).await
}

#[tauri::command]
async fn github_plugin_info(owner: Option<String>, repo: Option<String>) -> market::GithubInfo {
    market::github_plugin_info(
        owner.unwrap_or_default().trim(),
        repo.unwrap_or_default().trim(),
    )
    .await
}

/// GitHub 源插件的 `prepare` 构建脚本被 pnpm 安全闸门拦着，
/// 需要把包名写进 `profiles/web/pnpm-workspace.yaml` 的 `allowBuilds`。
/// 渲染层已经先弹过供应链风险确认了，到这里才写。
fn ensure_allow_builds(pkg_name: &str) -> std::io::Result<()> {
    if pkg_name.is_empty() {
        return Ok(());
    }
    let ws = usage::dsh_home()
        .join("profiles")
        .join("web")
        .join("pnpm-workspace.yaml");
    let text = std::fs::read_to_string(&ws).unwrap_or_default();
    if text.contains(pkg_name) {
        return Ok(()); // 已经放行过
    }
    let prefix = if text.is_empty() || text.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let addition = format!("{prefix}allowBuilds:\n  - '{pkg_name}'\n");
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&ws)?;
    f.write_all(addition.as_bytes())
}

#[tauri::command]
async fn install_github_plugin(
    app: AppHandle,
    owner: Option<String>,
    repo: Option<String>,
) -> PluginResult {
    let owner = owner.unwrap_or_default().trim().to_string();
    let repo = repo.unwrap_or_default().trim().to_string();
    if owner.is_empty() || repo.is_empty() {
        return PluginResult {
            ok: false,
            error: Some("仓库信息不完整".into()),
        };
    }
    if busy_get(&app).is_some() {
        return PluginResult {
            ok: false,
            error: Some("有操作正在进行".into()),
        };
    }

    busy_set(&app, Some("plugin"));
    send_state(&app).await;
    logging::log(format!("准备安装 GitHub 插件 {owner}/{repo}..."));

    let info = market::github_plugin_info(&owner, &repo).await;
    if let Some(pkg) = info.pkg_name.as_deref() {
        if let Err(e) = ensure_allow_builds(pkg) {
            logging::log(format!("allowBuilds 写入失败: {e}"));
        }
    }

    let spec = format!("github:{owner}/{repo}");
    logging::log(format!(
        "安装 {owner}/{repo}（dsh plugin --profile web add {spec}）..."
    ));
    let code = plugins::run_plugin_command(&["add", &spec]).await;
    busy_set(&app, None);
    send_state(&app).await;

    if code == 0 {
        logging::log(format!("插件 {owner}/{repo} 安装完成，重启 DSH 后生效。"));
        PluginResult {
            ok: true,
            error: None,
        }
    } else {
        logging::log(format!(
            "插件 {owner}/{repo} 安装失败（退出码 {code}），见上方日志。"
        ));
        PluginResult {
            ok: false,
            error: Some(format!("退出码 {code}")),
        }
    }
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeResult {
    ok: bool,
    label: Option<String>,
    result: Option<serde_json::Value>,
    cached: bool,
    error: Option<String>,
}

#[tauri::command]
async fn plugin_analyze(
    app: AppHandle,
    source: Option<String>,
    #[allow(non_snake_case)] r#ref: Option<String>,
    force: Option<bool>,
) -> AnalyzeResult {
    let source = source.unwrap_or_else(|| "npm".into());
    let reference = r#ref.unwrap_or_default();
    let force = force.unwrap_or(false);
    if reference.is_empty() {
        return AnalyzeResult {
            error: Some("缺少插件标识".into()),
            ..Default::default()
        };
    }
    if busy_get(&app).is_some() {
        return AnalyzeResult {
            error: Some("busy".into()),
            ..Default::default()
        };
    }

    busy_set(&app, Some("analyze"));
    send_state(&app).await;
    let out = analyze_inner(&app, &source, &reference, force).await;
    busy_set(&app, None);
    send_state(&app).await;
    out
}

async fn analyze_inner(
    app: &AppHandle,
    source: &str,
    reference: &str,
    force: bool,
) -> AnalyzeResult {
    // ① 收集公开资料档案（headless 没有联网工具，所以由管理器来收）。
    let (label, dossier) = if source == "npm" {
        analysis::send_analyze_log(app, "正在收集插件档案（npm + GitHub）...");
        let Some(info) = market::npm_plugin_info(reference).await else {
            analysis::send_analyze_log(app, "获取插件信息失败（网络异常或包不存在）。");
            logging::log(format!("插件分析中止（{reference}）：获取 npm 信息失败。"));
            return AnalyzeResult {
                error: Some("info".into()),
                ..Default::default()
            };
        };
        (
            format!("{}@{}", info.name, info.version),
            analysis::build_npm_dossier(&info),
        )
    } else {
        let mut parts = reference.splitn(2, '/');
        let owner = parts.next().unwrap_or("");
        let repo = parts.next().unwrap_or("");
        analysis::send_analyze_log(app, "正在收集插件档案（GitHub）...");
        let info = market::github_plugin_info(owner, repo).await;
        if info.stats.is_none() {
            // 原因要落到日志文件里：分析控制台的内容只走事件、不入库，
            // 窗口一关就查不到了。
            let why = info.error.as_deref().unwrap_or("未知原因");
            let line = format!("获取仓库信息失败：{why}");
            analysis::send_analyze_log(app, &line);
            logging::log(format!("插件分析中止（{owner}/{repo}）：{line}"));
            return AnalyzeResult {
                error: Some("info".into()),
                ..Default::default()
            };
        }
        (
            format!("{owner}/{repo}"),
            analysis::build_github_dossier(&info),
        )
    };

    // ② 命中历史缓存就直接返回（分析要花真金白银的 API tokens）。
    if !force {
        if let Some(cached) = analysis::load_history(source, reference) {
            if let Some(result) = cached.get("result") {
                let when = cached
                    .get("analyzedAt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .chars()
                    .take(10)
                    .collect::<String>();
                analysis::send_analyze_log(
                    app,
                    format!("发现历史分析结果（{when}），如需重新评估请点「重新分析」。"),
                );
                analysis::send_analyze_done(app, result);
                return AnalyzeResult {
                    ok: true,
                    label: Some(label),
                    result: Some(result.clone()),
                    cached: true,
                    error: None,
                };
            }
        }
    }

    // ③ 补齐 headless profile 缺的模型适配器插件，否则会报 NO_ADAPTER。
    analysis::send_analyze_log(app, "检查分析环境（headless profile 插件）...");
    analysis::ensure_analysis_env(app).await;

    // ④ 跑 headless。
    let prefix = app.state::<AppState>().prefix();
    let task = analysis::prompt_for(&label, &dossier);
    analysis::send_analyze_log(
        app,
        format!("调用 dsh headless 分析 {label}（最长 10 分钟，会消耗 API tokens）..."),
    );
    let r = analysis::run_headless(app, &prefix, &task).await;
    analysis::send_analyze_log(app, format!("分析进程结束（退出码 {}）。", r.code));

    // ⑤ 分析结束后清理管理器私有的会话目录（绝不碰 $DSH_HOME/sessions）。
    let cfg = app.state::<AppState>().cfg();
    if cfg.clean_analysis_sessions {
        analysis::clean_analysis_sessions();
        analysis::send_analyze_log(app, "已清理分析会话目录（不会影响 dsh web 会话）。");
    }

    let result = match r.json {
        Some(json) => analysis::with_raw(json, &r.raw),
        None => {
            analysis::send_analyze_log(app, "未能从输出中解析出结构化结论，请查看上方原始输出。");
            analysis::fallback_result(&r.raw)
        }
    };
    analysis::save_history(source, reference, &label, &result);
    analysis::send_analyze_done(app, &result);
    AnalyzeResult {
        ok: true,
        label: Some(label),
        result: Some(result),
        cached: false,
        error: None,
    }
}

#[tauri::command]
fn plugin_analyze_stop(app: AppHandle) -> bool {
    analysis::stop(&app)
}

/// 令牌操作的结果。**不含令牌本身**，`status` 里只有掩码后的尾 4 位。
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TokenResult {
    ok: bool,
    /// 给用户看的一句话。
    message: String,
    status: token::Status,
}

#[tauri::command]
fn get_token_status() -> token::Status {
    token::status()
}

/// 保存令牌。校验形状 -> 联网验一次 -> 才写进凭据管理器。
#[tauri::command]
async fn set_github_token(token: Option<String>) -> TokenResult {
    let raw = token.unwrap_or_default();
    let t = raw.trim();
    let done = |ok: bool, message: String| TokenResult {
        ok,
        message,
        status: token::status(),
    };
    if let Err(e) = token::validate(t) {
        return done(false, e);
    }
    match gh::probe_token(t).await {
        Ok(limit) => match token::save(t) {
            Ok(()) => {
                logging::log(format!(
                    "已保存 GitHub 令牌（{}），配额 {limit} 次/小时。",
                    token::hint(t)
                ));
                done(true, format!("令牌有效，配额 {limit} 次/小时。"))
            }
            Err(e) => done(false, e),
        },
        // 令牌本身不对就别存，否则市场会继续以"限流"的面目失败。
        Err(e) if e.contains("401") => done(false, format!("{e}，没有保存。")),
        // 网络不通不该拦着保存 —— 用户很可能正是因为连不上/被限流才来配这个。
        Err(e) => match token::save(t) {
            Ok(()) => {
                logging::log(format!(
                    "已保存 GitHub 令牌（{}），但未能联网校验。",
                    token::hint(t)
                ));
                done(true, format!("已保存，但没能联网校验：{e}"))
            }
            Err(e2) => done(false, e2),
        },
    }
}

#[tauri::command]
fn clear_github_token() -> TokenResult {
    let status_after = |ok: bool, message: String| TokenResult {
        ok,
        message,
        status: token::status(),
    };
    match token::clear() {
        Ok(()) => {
            logging::log("已清除 GitHub 令牌。");
            status_after(true, "已清除。".into())
        }
        Err(e) => status_after(false, e),
    }
}

#[tauri::command]
fn analysis_history(source: Option<String>, r#ref: Option<String>) -> Option<serde_json::Value> {
    analysis::load_history(
        source.as_deref().unwrap_or("npm"),
        r#ref.as_deref().unwrap_or(""),
    )
}
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    ok: bool,
    file_path: Option<String>,
    error: Option<String>,
}

#[tauri::command]
async fn export_log(app: AppHandle, text: Option<String>) -> ExportResult {
    use tauri_plugin_dialog::DialogExt;

    let default_name = format!(
        "dsh-manager-log-{}.txt",
        chrono::Local::now().format("%Y-%m-%d")
    );
    let picked = app
        .dialog()
        .file()
        .set_title("导出日志")
        .set_file_name(&default_name)
        .add_filter("文本文件", &["txt", "log"])
        .blocking_save_file();

    let Some(path) = picked else {
        return ExportResult::default(); // 用户取消
    };
    let Ok(path) = path.into_path() else {
        return ExportResult {
            error: Some("无法解析所选路径".into()),
            ..Default::default()
        };
    };
    match std::fs::write(&path, text.unwrap_or_default()) {
        Ok(_) => ExportResult {
            ok: true,
            file_path: Some(path.to_string_lossy().into_owned()),
            error: None,
        },
        Err(e) => ExportResult {
            error: Some(e.to_string()),
            ..Default::default()
        },
    }
}

#[tauri::command]
fn create_shortcut(app: AppHandle) -> Result<(), String> {
    make_shortcut(&app, false)
}

/// 建桌面快捷方式。`only_if_missing` 给启动时的自动补建用。
///
/// **开发构建一律拒绝。** 真机踩过一次：在 `tauri dev` 里点了托盘的
/// "创建桌面快捷方式"，桌面上就留下一个指向 `target\debug\dsh-manager.exe`
/// 的 .lnk。那个产物有两处致命差别 —— 它是控制台子系统（双击先弹一个黑框），
/// 前端又指向 `tauri dev` 起的临时服务器，脱离 dev 双击只会得到 WebView2 的
/// "无法访问此页面"。更糟的是启动时的自动补建用的是 `only_if_missing`，
/// 发布版看见文件已存在就跳过，于是这个坏快捷方式会一直留着。
///
/// 启动路径原本就有 `!cfg!(debug_assertions)` 把关，漏的是托盘和命令这两条
/// 手动入口 —— 所以把判断挪到这里，一处管全部。
fn make_shortcut(app: &AppHandle, only_if_missing: bool) -> Result<(), String> {
    if cfg!(debug_assertions) {
        let msg = "开发构建不创建桌面快捷方式：debug 产物脱离 tauri dev 打不开（会弹控制台，页面也加载不出来）。要桌面图标请用 tauri build 的产物或安装器。";
        logging::log(msg);
        return Err(msg.to_string());
    }
    let desktop = app
        .path()
        .desktop_dir()
        .map_err(|e| format!("取不到桌面目录: {e}"))?;
    let exe = std::env::current_exe().map_err(|e| format!("取不到自身路径: {e}"))?;
    // Windows 的快捷方式图标认 ICO；exe 自带的图标资源就来自 bundle.icon 里的 icon.ico。
    match shortcut::create_desktop_shortcut(&desktop, &exe, &exe, only_if_missing) {
        Ok(Some(path)) => {
            logging::log(format!("已创建桌面快捷方式: {}", path.display()));
            Ok(())
        }
        Ok(None) => Ok(()), // 已存在，跳过
        Err(e) => {
            logging::log(format!("创建快捷方式失败: {e}"));
            Err(e)
        }
    }
}

// ---------------------------------------------------------------------------
// 外壳：窗口、托盘、主题、定时器
// ---------------------------------------------------------------------------

/// dsh CLI 自身的依赖集合。市场搜索把这些核心包排掉，
/// 否则用户会在"插件市场"里看到一堆 dsh 内部包。
fn load_core_packages(npm_prefix: &str) -> std::collections::HashSet<String> {
    let path = std::path::Path::new(npm_prefix)
        .join("node_modules")
        .join("@deepseek-ai")
        .join("dsh")
        .join("package.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| {
            v.get("dependencies")?
                .as_object()
                .map(|o| o.keys().cloned().collect())
        })
        .unwrap_or_default()
}

fn busy_get(app: &AppHandle) -> Option<String> {
    app.state::<AppState>()
        .busy
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

fn busy_set(app: &AppHandle, value: Option<&str>) {
    *app.state::<AppState>()
        .busy
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = value.map(|s| s.to_string());
}

/// 改几个配置字段并立刻落盘（内部调用用，不经过渲染层的 set_config）。
fn set_config_fields(app: &AppHandle, f: impl FnOnce(&mut Config)) {
    let st = app.state::<AppState>();
    let snapshot = {
        let mut guard = st.config.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard);
        guard.clone()
    };
    let _ = config::save(&snapshot);
}

enum ProgressKind {
    /// 不知道还要多久：任务栏显示滚动条（对应 Electron 的 setProgressBar(2)）。
    Indeterminate,
}

fn set_progress(app: &AppHandle, kind: Option<ProgressKind>) {
    use tauri::window::{ProgressBarState, ProgressBarStatus};
    let state = match kind {
        Some(ProgressKind::Indeterminate) => ProgressBarState {
            status: Some(ProgressBarStatus::Indeterminate),
            progress: None,
        },
        None => ProgressBarState {
            status: Some(ProgressBarStatus::None),
            progress: None,
        },
    };
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.set_progress_bar(state);
    }
}

/// 查一次管理器自身有没有新版本，有就记日志 + 弹通知。
///
/// **只检查，不自动安装**（决策 4，见 docs 第十六节）。这也是 `autoUpdateManager`
/// 这个开关真正落地的地方 —— 在此之前它在配置结构和界面上都存在，后端却没人读，
/// 点了毫无反应。
///
/// debug 构建跳过：开发时 `CARGO_PKG_VERSION` 是仓库里的占位版本，
/// 每次启动都会报"有新版"，纯噪音。
async fn check_manager_update(app: &AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    let (enabled, channel, lang) = {
        let st = app.state::<AppState>();
        let cfg = st.config.lock().unwrap_or_else(|e| e.into_inner());
        (
            cfg.auto_update_manager,
            cfg.update_channel.clone(),
            cfg.language.clone(),
        )
    };
    if !enabled {
        return;
    }
    match updates::latest_manager_release(&channel).await {
        Ok(Some(v)) => {
            logging::log(format!(
                "管理器有新版本 v{v}，请到 {} 手动下载安装。",
                updates::MANAGER_RELEASES_PAGE
            ));
            notify(
                app,
                &texts::t(&lang, texts::Key::NotifyManagerUpdateTitle),
                &texts::notify_manager_update_body(&lang, &v),
            );
        }
        Ok(None) => {}
        // 查不到不值得打扰用户（大概率是限流或断网），但要留痕，
        // 否则"为什么从来没提示过新版"就无从排查。
        Err(e) => logging::log(format!("检查管理器更新失败：{e}")),
    }
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title(title).body(body).show();
}

/// 全局快捷键 Ctrl+Alt+D：唤起窗口并打开 DSH（实际动作在插件的 handler 里）。
fn register_global_shortcut(app: &AppHandle) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    if let Err(e) = app.global_shortcut().register("CmdOrCtrl+Alt+D") {
        // 被别的程序占用是常见情况，记一条日志就够，不影响主流程。
        logging::log(format!("注册全局快捷键 Ctrl+Alt+D 失败: {e}"));
    }
}

fn apply_autostart(app: &AppHandle, enabled: bool) {
    use tauri_plugin_autostart::ManagerExt;
    let mgr = app.autolaunch();

    // 先查当前状态：对一个本来就不存在的自启项调 disable()，插件会报
    // "系统找不到指定的文件 (os error 2)" —— 启动烟测里每次都刷这条，
    // 看着像坏了其实没事。只在状态需要改变时才动手。
    match mgr.is_enabled() {
        Ok(current) if current == enabled => return,
        Ok(_) => {}
        Err(e) => {
            // 查不到状态就按原样尝试设置，别因为查询失败就放弃。
            logging::log(format!("读取开机自启状态失败（继续尝试设置）: {e}"));
        }
    }

    let r = if enabled { mgr.enable() } else { mgr.disable() };
    if let Err(e) = r {
        logging::log(format!("设置开机自启失败: {e}"));
    }
}

/// 自动检查更新的定时器代数。改间隔时递增，旧任务发现代数变了就自己退出
/// —— 比持有 JoinHandle 去 abort 简单，且不会漏掉已经在 sleep 里的任务。
static AUTO_CHECK_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn restart_auto_check_timer(app: &AppHandle, cfg: &Config) {
    use std::sync::atomic::Ordering;
    let generation = AUTO_CHECK_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    if !cfg.auto_check_on_startup || cfg.auto_check_interval_hours == 0 {
        return;
    }
    let hours = cfg.auto_check_interval_hours;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(hours * 3600)).await;
            if AUTO_CHECK_GEN.load(Ordering::SeqCst) != generation {
                return; // 已被新定时器取代
            }
            check_updates(app.clone(), Some(true)).await;
        }
    });
}

fn open_url(app: &AppHandle, url: &str) {
    use tauri_plugin_opener::OpenerExt;
    let _ = app.opener().open_url(url, None::<&str>);
}

fn open_path(app: &AppHandle, path: &str) {
    use tauri_plugin_opener::OpenerExt;
    let _ = app.opener().open_path(path, None::<&str>);
}

fn apply_theme(app: &AppHandle, theme: &str) {
    let t = match theme {
        "light" => Some(tauri::Theme::Light),
        "dark" => Some(tauri::Theme::Dark),
        _ => None, // 跟随系统
    };
    for win in app.webview_windows().values() {
        let _ = win.set_theme(t);
    }
}

fn create_main_window(app: &AppHandle, cfg: &Config) -> tauri::Result<()> {
    let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title(texts::t(&cfg.language, texts::Key::AppTitle))
        .inner_size(800.0, 700.0)
        .min_inner_size(720.0, 600.0)
        .visible(false);

    // 记忆窗口位置/大小：只在尺寸合理且至少有一块屏能看见时才恢复，
    // 否则窗口会落到已拔掉的显示器上（Electron 版 isBoundsVisible 的意思）。
    if let Some(b) = &cfg.window_bounds {
        if b.width >= 720 && b.height >= 600 {
            builder = builder
                .inner_size(b.width as f64, b.height as f64)
                .position(b.x as f64, b.y as f64);
        }
    }

    let win = builder.build()?;
    if !cfg.minimize_to_tray_on_startup {
        let _ = win.show();
    }
    Ok(())
}

fn show_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}

fn update_tray(app: &AppHandle, proc_count: usize) {
    let cfg = app.state::<AppState>().cfg();
    let tip = if proc_count > 0 {
        texts::tray_tooltip_running(&cfg.language, proc_count)
    } else {
        texts::t(&cfg.language, texts::Key::TrayTooltip)
    };
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(&tip));
    }
}

fn build_tray(app: &AppHandle, cfg: &Config) -> tauri::Result<()> {
    let lang = &cfg.language;
    let open = MenuItem::with_id(
        app,
        "open",
        texts::t(lang, texts::Key::TrayOpen),
        true,
        None::<&str>,
    )?;
    let open_dsh_i = MenuItem::with_id(
        app,
        "open-dsh",
        texts::t(lang, texts::Key::TrayOpenDsh),
        true,
        None::<&str>,
    )?;
    let restart = MenuItem::with_id(
        app,
        "restart",
        texts::t(lang, texts::Key::TrayRestart),
        true,
        None::<&str>,
    )?;
    let stop = MenuItem::with_id(
        app,
        "stop",
        texts::t(lang, texts::Key::TrayStop),
        true,
        None::<&str>,
    )?;
    let check = MenuItem::with_id(
        app,
        "check",
        texts::t(lang, texts::Key::TrayCheck),
        true,
        None::<&str>,
    )?;
    let shortcut_item = MenuItem::with_id(
        app,
        "shortcut",
        texts::t(lang, texts::Key::TrayShortcut),
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(
        app,
        "quit",
        texts::t(lang, texts::Key::TrayQuit),
        true,
        None::<&str>,
    )?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &open_dsh_i,
            &restart,
            &stop,
            &check,
            &shortcut_item,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id("main")
        .icon(
            app.default_window_icon()
                .cloned()
                .expect("打包时一定有默认图标"),
        )
        .tooltip(texts::t(lang, texts::Key::TrayTooltip))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let app = app.clone();
            match event.id().as_ref() {
                "open" => show_main_window(&app),
                "open-dsh" => {
                    tauri::async_runtime::spawn(async move {
                        let _ = open_dsh(app).await;
                    });
                }
                "restart" => {
                    tauri::async_runtime::spawn(async move {
                        let _ = restart_dsh(app).await;
                    });
                }
                "stop" => {
                    tauri::async_runtime::spawn(async move {
                        stop_dsh(app, None).await;
                    });
                }
                "check" => {
                    tauri::async_runtime::spawn(async move {
                        check_updates(app, Some(false)).await;
                    });
                }
                "shortcut" => {
                    let _ = make_shortcut(&app, false);
                }
                "quit" => {
                    logging::flush_child_repeat();
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// 状态轮询：窗口可见 6 秒、隐藏到托盘 30 秒（与 Electron 版一致，省资源）。
fn spawn_state_timer(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let visible = app
                .get_webview_window("main")
                .and_then(|w| w.is_visible().ok())
                .unwrap_or(false);
            tokio::time::sleep(Duration::from_secs(if visible { 6 } else { 30 })).await;
            send_state(&app).await;
        }
    });
}

/// 日志清理：除启动时那次之外每 6 小时再跑一次。
/// 只在启动时清一次的话，托盘常驻应用连跑数周就等于没有保留策略。
fn spawn_log_cleanup_timer() {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
            logging::cleanup_old_logs();
        }
    });
}

/// 守护：dsh 异常退出后自动重启（10 分钟内最多 3 次，防崩溃循环）。
fn spawn_watchdog(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut restarts: Vec<std::time::Instant> = Vec::new();
        loop {
            tokio::time::sleep(Duration::from_secs(8)).await;
            let cfg = app.state::<AppState>().cfg();
            if !cfg.watchdog {
                continue;
            }
            let info = dsh::child_info();
            if info.alive || !info.exited_unexpectedly {
                continue;
            }
            restarts.retain(|t| t.elapsed() < Duration::from_secs(600));
            if restarts.len() >= 3 {
                logging::log("DSH 在 10 分钟内多次退出，已暂停自动重启，请手动检查。");
                dsh::clear_child(true);
                continue;
            }
            restarts.push(std::time::Instant::now());
            logging::log("检测到 DSH 异常退出，自动重启...");
            dsh::clear_child(true);
            let _ = restart_dsh(app.clone()).await;
        }
    });
}

/// panic 时留一份 crash.log。
///
/// 对应 Electron 版的 `uncaughtException -> crash.log`：没有这个兜底，
/// 用户遇到的就是"双击了没反应"，什么线索都不剩。release 配了 `panic = "abort"`，
/// 但 hook 在 abort 之前执行，所以照样能落盘。
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let text = format!(
            "[{}] panic: {info}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        );
        if std::fs::create_dir_all(config::config_dir()).is_ok() {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(config::config_dir().join("crash.log"))
            {
                let _ = f.write_all(text.as_bytes());
            }
        }
        default(info);
    }));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    install_panic_hook();
    let cfg = config::load();

    let mut builder = tauri::Builder::default();

    // 单实例：第二次启动只唤醒已有窗口（Electron 版靠 requestSingleInstanceLock）。
    #[cfg(windows)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main_window(app);
        }));
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            // 打包版 exe 本身就是应用，自启不需要额外参数。
            None,
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    // 只在按下时响应，否则一次按键会触发两遍。
                    if event.state() != tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        return;
                    }
                    let app = app.clone();
                    show_main_window(&app);
                    tauri::async_runtime::spawn(async move {
                        let _ = open_dsh(app).await;
                    });
                })
                .build(),
        )
        .manage(AppState::new(cfg.clone()))
        .invoke_handler(tauri::generate_handler![
            get_state,
            open_dsh,
            stop_dsh,
            restart_dsh,
            get_recent_logs,
            log_frontend_error,
            set_config,
            open_config_dir,
            open_npm_dir,
            open_external,
            open_market,
            check_updates,
            update,
            rollback,
            get_changelog,
            export_log,
            create_shortcut,
            get_usage,
            get_plugins,
            check_plugin_updates,
            install_plugin,
            remove_plugin,
            upgrade_plugin,
            market_search,
            plugin_info,
            github_plugin_info,
            install_github_plugin,
            plugin_analyze,
            plugin_analyze_stop,
            analysis_history,
            get_token_status,
            set_github_token,
            clear_github_token,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            logging::init(handle.clone());
            logging::cleanup_old_logs();

            create_main_window(&handle, &cfg)?;
            build_tray(&handle, &cfg)?;
            apply_theme(&handle, &cfg.theme);
            apply_autostart(&handle, cfg.auto_start_with_windows);
            register_global_shortcut(&handle);
            // 调试构建不自动建桌面快捷方式：`tauri dev` 的产物在 target/debug 下，
            // 随时会被 cargo clean 掉，往用户桌面放一个指向它的链接纯属污染。
            // 托盘菜单里那个「创建桌面快捷方式」仍然可用（用户明确要求时才建）。
            // debug 的把关在 make_shortcut 里，这里不再重复判断。
            if cfg.create_desktop_shortcut {
                let _ = make_shortcut(&handle, true);
            }

            // 清理上次崩溃遗留的分析会话（只动管理器私有目录）。
            if cfg.clean_analysis_sessions {
                analysis::clean_analysis_sessions();
            }
            analysis::ensure_session_patch();

            // npm 前缀要起子进程解析，别卡住启动。核心包集合、补丁行校验、
            // 首次检查更新都挂在它后面，因为都要用到这个前缀。
            let h = handle.clone();
            let auto_check = cfg.auto_check_on_startup;
            tauri::async_runtime::spawn(async move {
                let prefix = dsh::resolve_npm_prefix().await;
                if prefix.is_empty() {
                    logging::log(
                        "未能解析 npm 全局前缀（npm prefix -g），版本与插件功能可能不可用。",
                    );
                } else {
                    let st = h.state::<AppState>();
                    *st.core_packages.lock().unwrap_or_else(|e| e.into_inner()) =
                        load_core_packages(&prefix);
                    *st.npm_prefix.lock().unwrap_or_else(|e| e.into_inner()) = prefix.clone();
                    // dsh 升级若改了补丁行 id，会话隔离会静默失效，这里告警。
                    analysis::verify_patch_row(&prefix);
                }
                send_state(&h).await;
                if auto_check {
                    check_updates(h.clone(), Some(true)).await;
                }
                check_manager_update(&h).await;
            });

            spawn_state_timer(handle.clone());
            spawn_log_cleanup_timer();
            spawn_watchdog(handle.clone());
            restart_auto_check_timer(&handle, &cfg);
            Ok(())
        })
        .on_window_event(|win, event| {
            // 关闭窗口只是收进托盘（托盘常驻），顺手记住窗口大小位置。
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if win.label() == "main" {
                    api.prevent_close();
                    remember_bounds(win);
                    let _ = win.hide();
                    // 主窗口收进托盘时把市场窗口也一并关掉：否则用户以为"管理器关了"，
                    // 桌面上却还孤零零留着一个市场窗口。市场窗口没有未保存状态
                    // （滚动位置与分栏比例都在 localStorage 里），关掉无损失。
                    if let Some(market) = win.app_handle().get_webview_window("market") {
                        let _ = market.close();
                    }
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("Tauri 应用初始化失败")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                logging::flush_child_repeat();
                let cfg = app.state::<AppState>().cfg();
                let _ = config::save(&cfg);
            }
        });
}

fn remember_bounds(win: &tauri::Window) {
    let app = win.app_handle();
    let (Ok(pos), Ok(size)) = (win.outer_position(), win.inner_size()) else {
        return;
    };
    let st = app.state::<AppState>();
    let mut guard = st.config.lock().unwrap_or_else(|e| e.into_inner());
    guard.window_bounds = Some(config::WindowBounds {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
    });
    let snapshot = guard.clone();
    drop(guard);
    let _ = config::save(&snapshot);
}
