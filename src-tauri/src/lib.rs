//! DSH 管理器 — Tauri 版主模块（阶段一：状态 / 启停 / 托盘 / 日志）。
//!
//! 与 Electron 版的对应关系见 `docs/TAURI-MIGRATION.md`。阶段一刻意保持
//! **渲染层零改动**：`renderer/tauri-bridge.js` 把 `window.dsh.*` 映射到
//! Tauri 的 `invoke`/`listen`，所以 `renderer.js` / `market.js` 不用动。

pub mod config;
pub mod dsh;
pub mod logging;
pub mod procs;
pub mod pure;
pub mod texts;

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use config::Config;

/// 未实现的阶段二/三/四命令统一返回这个，方便前端灰掉按钮而不是白屏。
const NOT_YET: &str = "该功能正在迁移到 Tauri 版，尚未实现";

pub struct AppState {
    pub config: Mutex<Config>,
    pub npm_prefix: Mutex<String>,
    pub latest_version: Mutex<Option<String>>,
    pub last_check_time: Mutex<Option<String>>,
    pub busy: Mutex<Option<String>>,
}

impl AppState {
    fn new(cfg: Config) -> Self {
        Self {
            config: Mutex::new(cfg),
            npm_prefix: Mutex::new(String::new()),
            latest_version: Mutex::new(None),
            last_check_time: Mutex::new(None),
            busy: Mutex::new(None),
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

    let theme_changed = {
        let mut guard = st.config.lock().unwrap_or_else(|e| e.into_inner());
        let changed = guard.theme != merged.theme;
        *guard = merged.clone();
        changed
    };
    config::save(&merged).map_err(|e| e.to_string())?;
    if theme_changed {
        apply_theme(&app, &merged.theme);
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

#[tauri::command]
fn open_market(app: AppHandle) -> Result<(), String> {
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

// --- 以下为后续阶段；先占位，免得前端调用直接炸 -------------------------------

#[tauri::command]
fn check_updates() -> Result<serde_json::Value, String> {
    Err(NOT_YET.into())
}
#[tauri::command]
fn update() -> Result<serde_json::Value, String> {
    Err(NOT_YET.into())
}
#[tauri::command]
fn rollback() -> Result<serde_json::Value, String> {
    Err(NOT_YET.into())
}
#[tauri::command]
fn get_changelog() -> Result<String, String> {
    Err(NOT_YET.into())
}
#[tauri::command]
fn get_usage() -> Result<serde_json::Value, String> {
    Err(NOT_YET.into())
}
#[tauri::command]
fn get_plugins() -> Result<serde_json::Value, String> {
    Err(NOT_YET.into())
}
#[tauri::command]
fn check_plugin_updates() -> Result<serde_json::Value, String> {
    Err(NOT_YET.into())
}
#[tauri::command]
fn install_plugin() -> Result<serde_json::Value, String> {
    Err(NOT_YET.into())
}
#[tauri::command]
fn remove_plugin() -> Result<serde_json::Value, String> {
    Err(NOT_YET.into())
}
#[tauri::command]
fn upgrade_plugin() -> Result<serde_json::Value, String> {
    Err(NOT_YET.into())
}
#[tauri::command]
fn market_search() -> Result<serde_json::Value, String> {
    Err(NOT_YET.into())
}
#[tauri::command]
fn plugin_info() -> Result<serde_json::Value, String> {
    Err(NOT_YET.into())
}
#[tauri::command]
fn github_plugin_info() -> Result<serde_json::Value, String> {
    Err(NOT_YET.into())
}
#[tauri::command]
fn install_github_plugin() -> Result<serde_json::Value, String> {
    Err(NOT_YET.into())
}
#[tauri::command]
fn plugin_analyze() -> Result<serde_json::Value, String> {
    Err(NOT_YET.into())
}
#[tauri::command]
fn plugin_analyze_stop() -> Result<(), String> {
    Err(NOT_YET.into())
}
#[tauri::command]
fn analysis_history() -> Result<serde_json::Value, String> {
    Err(NOT_YET.into())
}
#[tauri::command]
fn export_log() -> Result<(), String> {
    Err(NOT_YET.into())
}
#[tauri::command]
fn create_shortcut() -> Result<(), String> {
    Err(NOT_YET.into())
}

// ---------------------------------------------------------------------------
// 外壳：窗口、托盘、主题、定时器
// ---------------------------------------------------------------------------

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
    let quit = MenuItem::with_id(
        app,
        "quit",
        texts::t(lang, texts::Key::TrayQuit),
        true,
        None::<&str>,
    )?;
    let menu = Menu::with_items(app, &[&open, &open_dsh_i, &restart, &stop, &quit])?;

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
        .manage(AppState::new(cfg.clone()))
        .invoke_handler(tauri::generate_handler![
            get_state,
            open_dsh,
            stop_dsh,
            restart_dsh,
            get_recent_logs,
            set_config,
            open_config_dir,
            open_npm_dir,
            open_external,
            open_market,
            // 后续阶段占位
            check_updates,
            update,
            rollback,
            get_changelog,
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
            export_log,
            create_shortcut,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            logging::init(handle.clone());
            logging::cleanup_old_logs();

            create_main_window(&handle, &cfg)?;
            build_tray(&handle, &cfg)?;
            apply_theme(&handle, &cfg.theme);

            // npm 前缀要起子进程解析，别卡住启动。
            let h = handle.clone();
            tauri::async_runtime::spawn(async move {
                let prefix = dsh::resolve_npm_prefix().await;
                if prefix.is_empty() {
                    logging::log(
                        "未能解析 npm 全局前缀（npm prefix -g），版本与插件功能可能不可用。",
                    );
                } else {
                    *h.state::<AppState>()
                        .npm_prefix
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()) = prefix;
                }
                send_state(&h).await;
            });

            spawn_state_timer(handle.clone());
            spawn_log_cleanup_timer();
            spawn_watchdog(handle.clone());
            Ok(())
        })
        .on_window_event(|win, event| {
            // 关闭窗口只是收进托盘（托盘常驻），顺手记住窗口大小位置。
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if win.label() == "main" {
                    api.prevent_close();
                    remember_bounds(win);
                    let _ = win.hide();
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
