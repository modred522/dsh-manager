'use strict';
// Shared UI dictionary for both windows (main + marketplace).
// Loaded BEFORE renderer.js / market.js.
const I18N = {
  zh: {
    // header
    appName: 'DSH 管理器',
    appSub: 'DeepSeek Harness 桌面管理器',
    // tabs
    tabHome: '总览', tabUsage: '用量', tabPlugins: '插件',
    // overview
    btnOpen: '打开 DSH', btnRestart: '重启 DSH', btnStopAll: '停止全部',
    btnCheck: '检查更新', btnUpdate: '立即更新',
    updateProgress: '正在更新 DSH…', updateProgressDetail: '正在更新 DSH…（详情见日志）',
    labelInstalled: '当前版本', labelLatest: '最新版本',
    serverChecking: 'DSH 服务状态：检测中…',
    serverUp: (u) => `DSH 服务状态：运行中（${u}）`,
    serverDown: (u) => `DSH 服务状态：未运行（${u}）`,
    procChecking: '检测中…', procTitle: 'DSH 进程', btnRefresh: '刷新',
    procCount: (n) => (n > 0 ? `检测到 ${n} 个 DSH 进程` : '未检测到 DSH 进程'),
    procNoCmd: '（未获取到命令行）', procStop: '停止',
    statusLatest: '已是最新', statusUpdate: '发现新版本', statusUnchecked: '未检查',
    unknown: '未知', lastChecked: (tm) => `上次检查 ${tm}`,
    rollback: (v) => `回滚 v${v}`,
    checkingNow: '检查中…', updatingNow: '更新中…', installingNow: '安装中…',
    // settings
    setAutoCheck: '自动检查更新', setWatchdog: '崩溃自动重启',
    setCleanAnalysis: '分析后清理分析会话', setSilent: '启动时静默到托盘',
    setAutoStart: '开机自动运行',
    setTheme: '主题', themeSystem: '跟随系统', themeLight: '浅色', themeDark: '深色',
    setLanguage: '语言', langSystem: '跟随系统', langZh: '中文', langEn: 'English',
    setUrl: 'DSH 地址', setInterval: '检查间隔(小时)',
    btnConfigDir: '配置目录', btnNpmDir: '安装目录', btnExportLog: '导出日志', btnAbout: '关于',
    logTitle: '日志', btnClearLog: '清空日志', logEmpty: '暂无日志',
    // usage
    statSessions: '总会话数', statOutput: '输出 tokens', statInput: '输入 tokens（未缓存）',
    statCache: '缓存读取 tokens', statCost: '估算费用（元）',
    projTitle: '按项目用量', dailyTitle: '近 14 天趋势',
    legendOutput: '输出', legendInput: '输入', legendCache: '缓存读取',
    projEmpty: '暂无会话数据',
    projVal: (o, t2, s) => `输出 ${o} · 共 ${t2} · ${s} 会话`,
    dayTip: (d, o, i, c) => `${d}  输出 ${o} · 输入 ${i} · 缓存 ${c}`,
    priceTitle: '费用单价（元 / 百万 tokens）', priceInput: '输入', priceCache: '缓存命中', priceOutput: '输出',
    priceNote: '按 DeepSeek 官网最新标价自行调整（当前为峰谷定价，此值为估算）',
    // plugins page
    marketEntryTitle: '插件市场', marketEntrySub: 'npm / GitHub 双源搜索 · 详情 · 一键分析（独立窗口）',
    btnOpenMarket: '打开插件市场',
    quickInstallTitle: '快速安装（npm）', quickInstallSub: 'web profile（重启 DSH 后生效）',
    pluginNamePh: 'npm 包名，如 @liustack/modlens', btnInstall: '安装',
    installedTitle: '已安装插件', installedEmpty: '暂无已安装插件', btnUninstall: '卸载',
    // update modal
    updateModalTitle: '发现新版本', updateModalSub: '更新内容（来自官方 Release）',
    btnUpdateNow: '立即更新', btnUpdateLater: '稍后再说', changelogEmpty: '（未获取到更新说明）',
    // about modal
    aboutTitle: '关于 DSH 管理器', aboutAppVersion: '管理器版本', aboutDshVersion: 'DSH 版本',
    aboutElectron: 'Electron', aboutNode: 'Node.js', aboutChromium: 'Chromium',
    aboutTip: '全局快捷键 Ctrl+Alt+D：快速打开 DSH', btnOk: '知道了',
    // toast texts
    toastRestarting: '正在重启 DSH…',
    toastStoppedProc: (n) => `已停止进程 ${n}`, toastProcGone: (n) => `进程 ${n} 已不存在`,
    toastStoppedAll: (n) => `已停止 ${n} 个 DSH 进程`, toastNoProcs: '没有运行中的 DSH 进程',
    toastLatest: '已是最新版本', toastCheckFailed: '检查失败，详见日志',
    toastNoUpdate: '暂无可用更新', toastUpdated: '更新完成', toastUpdateEnded: '更新结束，详见日志',
    toastLogExported: '日志已导出', toastExportFailed: '导出已取消或失败',
    toastRolledBack: (v) => `已回滚到 ${v}`, toastRollbackFailed: '回滚失败，详见日志',
    rollbackConfirm: (v) => `确定回滚到 v${v}？\n将执行 npm install -g @deepseek-ai/dsh@${v}`,
    toastUninstalled: (n) => `已卸载 ${n}，重启 DSH 生效`, toastUninstallFailed: '卸载失败，详见日志',
    toastPluginNameNeeded: '请输入插件包名',
    toastInstalled: (n) => `已安装 ${n}，重启 DSH 生效`, toastInstallFailed: '安装失败，详见日志',
    // marketplace window
    marketTitle: '插件市场', marketSub: 'npm / GitHub 双源搜索 · 详情 · 一键分析',
    btnSearch: '搜索',
    searchNpmPh: '搜索 dsh 插件（留空看热门）', searchGhPh: '搜索 GitHub 仓库（留空看热门）',
    toastSearchingNpm: '正在搜索 npm…', toastSearchingGh: '正在搜索 GitHub…',
    marketEmpty: '没有找到相关插件',
    badgeOfficial: '官方', badgeCommunity: '社区', badgeGithub: 'GitHub', badgeNpm: 'npm',
    badgeNoLicense: '无许可证', noDescription: '（无描述）', noReadme: '（无 README）',
    btnDetail: '详情', btnAnalyze: '分析',
    footerSearching: '正在搜索…', footerLoadingMore: '正在加载更多…', footerLoading: '正在加载…',
    footerScrollMore: '继续向下滚动加载更多…',
    footerAllLoaded: (n) => `已加载全部 ${n} 个插件`,
    footerRateLimited: '（部分源触发限流，稍后重试可查看更多）',
    footerFailNet: '加载失败（网络异常），点击重试', footerFailLimit: '加载失败（网络异常或触发限流），点击重试',
    toastRateLimited: '部分搜索源触发限流（GitHub 匿名接口 10 次/分钟），稍后重试可查看更多',
    // detail view
    btnBack: '← 返回市场', btnStopAnalyze: '停止分析',
    paneReadme: 'README（原始数据）', paneScore: '分析结果', paneLog: '分析控制台',
    loadingDetail: '加载中…',
    detailFailNpm: '获取详情失败（网络异常或包不存在）。',
    detailFailGh: '获取详情失败（可能被 GitHub 限流或仓库不存在）。',
    statDownloads: '周下载', statStars: 'Star', statIssues: 'Issues', statForks: 'Fork',
    statCreated: '创建', statUpdated: '更新', statPushed: '推送',
    analyzing: '分析中…', reanalyze: '重新分析',
    scoreEmpty: '尚未分析 —— 点击右上角「分析」生成评分卡',
    logEmptyHint: '分析控制台输出将显示在这里',
    scorePros: '优点', scoreCons: '缺点', scoreRisks: '风险',
    ghInstallConfirm: (ref) => `将从 GitHub 安装 ${ref}。\n\n该插件带安装期构建脚本（prepare），安装即允许其在本机执行代码——这是供应链攻击的常见入口。\n\n请确认你信任该仓库后继续。`,
  },
  en: {
    appName: 'DSH Manager',
    appSub: 'DeepSeek Harness Desktop Manager',
    tabHome: 'Overview', tabUsage: 'Usage', tabPlugins: 'Plugins',
    btnOpen: 'Open DSH', btnRestart: 'Restart DSH', btnStopAll: 'Stop All',
    btnCheck: 'Check Updates', btnUpdate: 'Update Now',
    updateProgress: 'Updating DSH…', updateProgressDetail: 'Updating DSH… (see logs)',
    labelInstalled: 'Installed', labelLatest: 'Latest',
    serverChecking: 'DSH service status: checking…',
    serverUp: (u) => `DSH service status: running (${u})`,
    serverDown: (u) => `DSH service status: stopped (${u})`,
    procChecking: 'Checking…', procTitle: 'DSH Processes', btnRefresh: 'Refresh',
    procCount: (n) => (n > 0 ? `${n} DSH process(es) detected` : 'No DSH processes detected'),
    procNoCmd: '(command line unavailable)', procStop: 'Stop',
    statusLatest: 'Up to date', statusUpdate: 'Update available', statusUnchecked: 'Not checked',
    unknown: 'Unknown', lastChecked: (tm) => `Last checked ${tm}`,
    rollback: (v) => `Rollback v${v}`,
    checkingNow: 'Checking…', updatingNow: 'Updating…', installingNow: 'Installing…',
    setAutoCheck: 'Auto check updates', setWatchdog: 'Auto restart on crash',
    setCleanAnalysis: 'Clean analysis sessions', setSilent: 'Start minimized to tray',
    setAutoStart: 'Run at startup',
    setTheme: 'Theme', themeSystem: 'System', themeLight: 'Light', themeDark: 'Dark',
    setLanguage: 'Language', langSystem: 'System', langZh: '中文', langEn: 'English',
    setUrl: 'DSH URL', setInterval: 'Check interval (h)',
    btnConfigDir: 'Config Folder', btnNpmDir: 'Install Folder', btnExportLog: 'Export Logs', btnAbout: 'About',
    logTitle: 'Logs', btnClearLog: 'Clear', logEmpty: 'No logs yet',
    statSessions: 'Total Sessions', statOutput: 'Output Tokens', statInput: 'Input Tokens (uncached)',
    statCache: 'Cache Read Tokens', statCost: 'Est. Cost (CNY)',
    projTitle: 'Usage by Project', dailyTitle: 'Last 14 Days',
    legendOutput: 'Output', legendInput: 'Input', legendCache: 'Cache read',
    projEmpty: 'No session data yet',
    projVal: (o, t2, s) => `Output ${o} · Total ${t2} · ${s} sessions`,
    dayTip: (d, o, i, c) => `${d}  Output ${o} · Input ${i} · Cache ${c}`,
    priceTitle: 'Unit Price (CNY / 1M tokens)', priceInput: 'Input', priceCache: 'Cache hit', priceOutput: 'Output',
    priceNote: 'Adjust to the latest DeepSeek pricing (peak/off-peak; estimate only)',
    marketEntryTitle: 'Plugin Marketplace', marketEntrySub: 'npm / GitHub search · details · one-click analysis (standalone window)',
    btnOpenMarket: 'Open Marketplace',
    quickInstallTitle: 'Quick Install (npm)', quickInstallSub: 'web profile (restart DSH to apply)',
    pluginNamePh: 'npm package name, e.g. @liustack/modlens', btnInstall: 'Install',
    installedTitle: 'Installed Plugins', installedEmpty: 'No plugins installed', btnUninstall: 'Remove',
    updateModalTitle: 'New Version Available', updateModalSub: 'Release notes (from the official release)',
    btnUpdateNow: 'Update Now', btnUpdateLater: 'Later', changelogEmpty: '(no release notes)',
    aboutTitle: 'About DSH Manager', aboutAppVersion: 'Manager Version', aboutDshVersion: 'DSH Version',
    aboutElectron: 'Electron', aboutNode: 'Node.js', aboutChromium: 'Chromium',
    aboutTip: 'Global shortcut Ctrl+Alt+D: open DSH quickly', btnOk: 'OK',
    toastRestarting: 'Restarting DSH…',
    toastStoppedProc: (n) => `Process ${n} stopped`, toastProcGone: (n) => `Process ${n} no longer exists`,
    toastStoppedAll: (n) => `Stopped ${n} DSH process(es)`, toastNoProcs: 'No DSH processes running',
    toastLatest: 'Already up to date', toastCheckFailed: 'Check failed, see logs',
    toastNoUpdate: 'No update available', toastUpdated: 'Update complete', toastUpdateEnded: 'Update finished, see logs',
    toastLogExported: 'Logs exported', toastExportFailed: 'Export cancelled or failed',
    toastRolledBack: (v) => `Rolled back to ${v}`, toastRollbackFailed: 'Rollback failed, see logs',
    rollbackConfirm: (v) => `Roll back to v${v}?\nThis runs: npm install -g @deepseek-ai/dsh@${v}`,
    toastUninstalled: (n) => `Uninstalled ${n}; restart DSH to apply`, toastUninstallFailed: 'Uninstall failed, see logs',
    toastPluginNameNeeded: 'Please enter a package name',
    toastInstalled: (n) => `Installed ${n}; restart DSH to apply`, toastInstallFailed: 'Install failed, see logs',
    marketTitle: 'Plugin Marketplace', marketSub: 'npm / GitHub search · details · one-click analysis',
    btnSearch: 'Search',
    searchNpmPh: 'Search dsh plugins (empty = trending)', searchGhPh: 'Search GitHub repos (empty = trending)',
    toastSearchingNpm: 'Searching npm…', toastSearchingGh: 'Searching GitHub…',
    marketEmpty: 'No matching plugins found',
    badgeOfficial: 'Official', badgeCommunity: 'Community', badgeGithub: 'GitHub', badgeNpm: 'npm',
    badgeNoLicense: 'No license', noDescription: '(no description)', noReadme: '(no README)',
    btnDetail: 'Details', btnAnalyze: 'Analyze',
    footerSearching: 'Searching…', footerLoadingMore: 'Loading more…', footerLoading: 'Loading…',
    footerScrollMore: 'Scroll down to load more…',
    footerAllLoaded: (n) => `All ${n} plugins loaded`,
    footerRateLimited: ' (some sources rate-limited; retry later for more)',
    footerFailNet: 'Load failed (network error), click to retry', footerFailLimit: 'Load failed (network error or rate limit), click to retry',
    toastRateLimited: 'Some search sources hit rate limits (anonymous GitHub API: 10/min); retry later for more results',
    btnBack: '← Back to Marketplace', btnStopAnalyze: 'Stop Analysis',
    paneReadme: 'README (raw data)', paneScore: 'Analysis Result', paneLog: 'Analysis Console',
    loadingDetail: 'Loading…',
    detailFailNpm: 'Failed to load details (network error or package not found).',
    detailFailGh: 'Failed to load details (possibly GitHub rate-limited or repo not found).',
    statDownloads: 'Downloads/wk', statStars: 'Stars', statIssues: 'Issues', statForks: 'Forks',
    statCreated: 'Created', statUpdated: 'Updated', statPushed: 'Pushed',
    analyzing: 'Analyzing…', reanalyze: 'Re-analyze',
    scoreEmpty: 'Not analyzed yet — click "Analyze" (top right) to generate a score card',
    logEmptyHint: 'Analysis console output will appear here',
    scorePros: 'Pros', scoreCons: 'Cons', scoreRisks: 'Risks',
    ghInstallConfirm: (ref) => `Install ${ref} from GitHub.\n\nThis plugin runs build scripts (prepare) at install time — executing code on this machine is a common supply-chain attack vector.\n\nContinue only if you trust this repository.`,
  },
};

let i18nLang = 'zh';

function setI18nLang(lang) {
  i18nLang = lang === 'en' ? 'en' : 'zh';
}

function t(key, ...args) {
  const d = I18N[i18nLang] || I18N.zh;
  const v = d[key] !== undefined ? d[key] : I18N.zh[key];
  if (typeof v === 'function') return v(...args);
  return v !== undefined ? v : key;
}

function applyI18nStatic() {
  document.querySelectorAll('[data-i18n]').forEach((el) => {
    el.textContent = t(el.getAttribute('data-i18n'));
  });
  document.querySelectorAll('[data-i18n-ph]').forEach((el) => {
    el.placeholder = t(el.getAttribute('data-i18n-ph'));
  });
  document.querySelectorAll('[data-i18n-title]').forEach((el) => {
    el.title = t(el.getAttribute('data-i18n-title'));
  });
  document.documentElement.lang = i18nLang === 'zh' ? 'zh-CN' : 'en';
}

function resolveUiLang(cfg) {
  if (cfg && (cfg.language === 'zh' || cfg.language === 'en')) return cfg.language;
  try {
    return String(navigator.language || '').toLowerCase().startsWith('zh') ? 'zh' : 'en';
  } catch {
    return 'en';
  }
}
