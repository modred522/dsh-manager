'use strict';
// Tauri 后端适配层：把 Electron preload 暴露的 `window.dsh` 契约原样架在
// Tauri 的 invoke/listen 之上，好处是 renderer.js / market.js / i18n.js
// **一行都不用改**，同一份渲染层能同时跑在两种后端上。
//
// 两种情况下自动让路：
//   * `window.dsh` 已存在  -> Electron 的 preload 已经注入，这里不插手
//   * 没有 `window.__TAURI__` -> 不在 Tauri 环境（Electron 构建会走到这里）
//
// 通道命名：Electron 用 kebab-case（'get-state'），Tauri 命令用 snake_case
// （`get_state`）。映射只在这一层发生，两边各自保持自己的惯例。

(function () {
  if (window.dsh || !window.__TAURI__) return;

  const { invoke } = window.__TAURI__.core;
  const { listen } = window.__TAURI__.event;

  // 事件回调：Electron 版的 onXxx 可被多次调用注册多个回调，这里保持同样语义。
  const on = (name) => (cb) => {
    listen(name, (e) => cb(e.payload));
  };

  window.dsh = {
    // --- 状态与进程 ---
    getState: () => invoke('get_state'),
    openDsh: () => invoke('open_dsh'),
    restartDsh: () => invoke('restart_dsh'),
    stopDsh: (pids) => invoke('stop_dsh', { pids: pids || null }),

    // --- 更新 ---
    checkUpdates: (silent) => invoke('check_updates', { silent: !!silent }),
    update: () => invoke('update'),
    rollback: () => invoke('rollback'),
    getChangelog: (version) => invoke('get_changelog', { version: version || null }),

    // --- 日志与工具 ---
    getRecentLogs: () => invoke('get_recent_logs'),
    exportLog: (text) => invoke('export_log', { text }),
    openConfigDir: () => invoke('open_config_dir'),
    openNpmDir: () => invoke('open_npm_dir'),
    createShortcut: () => invoke('create_shortcut'),
    setConfig: (cfg) => invoke('set_config', { cfg }),

    // --- 用量与插件 ---
    getUsage: () => invoke('get_usage'),
    getPlugins: () => invoke('get_plugins'),
    checkPluginUpdates: () => invoke('check_plugin_updates'),
    installPlugin: (name) => invoke('install_plugin', { name }),
    removePlugin: (name) => invoke('remove_plugin', { name }),
    upgradePlugin: (name) => invoke('upgrade_plugin', { name }),

    // --- 市场 ---
    marketSearch: (source, query, reset) => invoke('market_search', { source, query, reset: !!reset }),
    openMarket: () => invoke('open_market'),
    openExternal: (url) => invoke('open_external', { url }),
    pluginInfo: (name) => invoke('plugin_info', { name }),
    githubPluginInfo: (owner, repo) => invoke('github_plugin_info', { owner, repo }),
    installGithubPlugin: (owner, repo) => invoke('install_github_plugin', { owner, repo }),

    // --- 分析 ---
    pluginAnalyze: (source, ref, force) => invoke('plugin_analyze', { source, ref, force: !!force }),
    pluginAnalyzeStop: () => invoke('plugin_analyze_stop'),
    analysisHistory: (source, ref) => invoke('analysis_history', { source, ref }),

    // --- 事件（main -> renderer）---
    onLog: on('log'),
    onState: on('state'),
    onAnalyzeLog: on('analyze-log'),
    onAnalyzeDone: on('analyze-done'),
  };

  // 迁移期间部分命令还没实现（后端返回"尚未实现"）。渲染层的调用点大多没接
  // catch，不兜底的话用户只会看到页面一片空白、控制台里一条 unhandled rejection。
  // 这里把它显示到界面的日志区，让"还没做"和"坏了"能区分开。
  window.addEventListener('unhandledrejection', (e) => {
    const msg = String((e.reason && e.reason.message) || e.reason || '');
    if (!msg) return;
    const box = document.getElementById('log');
    if (box) {
      const row = document.createElement('div');
      row.className = 'log-line';
      row.textContent = `[迁移中] ${msg}`;
      box.appendChild(row);
      box.scrollTop = box.scrollHeight;
    }
    e.preventDefault();
  });
})();
