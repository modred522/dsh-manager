'use strict';

const $ = (id) => document.getElementById(id);

const els = {
  installed: $('installed'),
  latest: $('latest'),
  lastCheck: $('lastCheck'),
  statusText: $('statusText'),
  serverDot: $('serverDot'),
  serverState: $('serverState'),
  procCount: $('procCount'),
  btnOpen: $('btnOpen'),
  btnRestart: $('btnRestart'),
  btnStop: $('btnStop'),
  btnCheck: $('btnCheck'),
  btnUpdate: $('btnUpdate'),
  updateProgress: $('updateProgress'),
  updateStatus: $('updateStatus'),
  procsCard: $('procsCard'),
  procList: $('procList'),
  btnRefresh: $('btnRefresh'),
  chkAutoCheck: $('chkAutoCheck'),
  chkAutoStart: $('chkAutoStart'),
  chkWatchdog: $('chkWatchdog'),
  chkCleanAnalysis: $('chkCleanAnalysis'),
  chkSilent: $('chkSilent'),
  selTheme: $('selTheme'),
  selLang: $('selLang'),
  btnRollback: $('btnRollback'),
  txtUrl: $('txtUrl'),
  txtInterval: $('txtInterval'),
  btnConfigDir: $('btnConfigDir'),
  btnNpmDir: $('btnNpmDir'),
  btnExportLog: $('btnExportLog'),
  btnAbout: $('btnAbout'),
  btnClearLog: $('btnClearLog'),
  log: $('log'),
  modalUpdate: $('modalUpdate'),
  updateVersion: $('updateVersion'),
  changelog: $('changelog'),
  btnUpdateNow: $('btnUpdateNow'),
  btnUpdateLater: $('btnUpdateLater'),
  modalUpdateClose: $('modalUpdateClose'),
  modalAbout: $('modalAbout'),
  aboutBody: $('aboutBody'),
  btnAboutClose: $('btnAboutClose'),
  btnAboutOk: $('btnAboutOk'),
  toasts: $('toasts'),
  tabHome: $('tabHome'),
  tabUsage: $('tabUsage'),
  tabPlugins: $('tabPlugins'),
  pageHome: $('pageHome'),
  pageUsage: $('pageUsage'),
  pagePlugins: $('pagePlugins'),
  statSessions: $('statSessions'),
  statOutput: $('statOutput'),
  statInput: $('statInput'),
  statCache: $('statCache'),
  statCost: $('statCost'),
  projectBars: $('projectBars'),
  dailyChart: $('dailyChart'),
  btnUsageRefresh: $('btnUsageRefresh'),
  priceInput: $('priceInput'),
  priceCache: $('priceCache'),
  priceOutput: $('priceOutput'),
  pluginList: $('pluginList'),
  pluginName: $('pluginName'),
  btnInstallPlugin: $('btnInstallPlugin'),
  btnPluginsRefresh: $('btnPluginsRefresh'),
  btnPluginsCheck: $('btnPluginsCheck'),
  btnMarketOpen: $('btnMarketOpen'),
};

let busy = null;
let latestVersion = null;
let installedVersion = null;
let lastCount = -1;
let lastConfig = null;
let activeTab = 'home';
let rollbackTarget = null;
let lastUiLang = '';
let pluginUpdateMap = {};
let pluginUpdatesChecked = false;

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
}

// ---------------- 日志（带行上限 + 入场动画） ----------------
function appendLog(line) {
  const div = el('div', 'log-line', line);
  els.log.appendChild(div);
  while (els.log.childElementCount > 1000) els.log.removeChild(els.log.firstChild);
  els.log.scrollTop = els.log.scrollHeight;
}

// ---------------- Toast ----------------
function showToast(msg, kind = 'info') {
  const t = el('div', 'toast ' + kind, msg);
  els.toasts.appendChild(t);
  setTimeout(() => {
    t.classList.add('leaving');
    setTimeout(() => t.remove(), 320);
  }, 3000);
}

// ---------------- 格式化 ----------------
function fmtTokens(n) {
  n = Number(n) || 0;
  if (i18nLang === 'en') {
    if (n >= 1e9) return (n / 1e9).toFixed(2) + 'B';
    if (n >= 1e6) return (n / 1e6).toFixed(2) + 'M';
    if (n >= 1e3) return (n / 1e3).toFixed(1) + 'K';
    return n.toLocaleString('en-US');
  }
  if (n >= 1e8) return (n / 1e8).toFixed(2) + ' 亿';
  if (n >= 1e4) return (n / 1e4).toFixed(1) + ' 万';
  return n.toLocaleString('zh-CN');
}

function fmtCost(x) {
  return '¥' + (Number(x) || 0).toFixed(2);
}

function fmtTime(iso) {
  if (!iso) return '';
  const d = new Date(iso);
  if (isNaN(d.getTime())) return '';
  return d.toLocaleTimeString('zh-CN', { hour12: false });
}

// ---------------- 状态 ----------------
function setStatus(text, kind) {
  els.statusText.textContent = text;
  els.statusText.className = 'status ' + (kind || 'muted');
}

function setButtonLoading(btn, loading, loadingText, normalText) {
  btn.classList.toggle('loading', loading);
  btn.textContent = loading ? loadingText : normalText;
}

function updateProcCount(n) {
  els.procCount.textContent = t('procCount', n);
  if (n !== lastCount) {
    lastCount = n;
    els.procCount.classList.remove('pop');
    void els.procCount.offsetWidth;
    els.procCount.classList.add('pop');
  }
}

// ---------------- 进程面板 ----------------
function renderProcesses(procs) {
  els.procList.innerHTML = '';
  const list = procs || [];
  els.procsCard.hidden = list.length === 0;
  for (const p of list) {
    const row = el('div', 'proc-row');
    const cmd = el('span', 'proc-cmd', p.commandLine || t('procNoCmd'));
    if (p.commandLine) cmd.title = p.commandLine;
    const stopBtn = el('button', 'btn btn-small btn-danger', t('procStop'));
    stopBtn.addEventListener('click', () => stopOne(p.pid));
    const resText = [];
    if (p.memMb) resText.push(p.memMb + ' MB');
    if (p.cpuPercent != null) resText.push('CPU ' + p.cpuPercent + '%');
    row.append(
      el('span', 'dot' + (p.listening ? ' up' : '')),
      el('span', 'proc-pid', 'PID ' + p.pid),
      cmd,
      el('span', 'proc-res', resText.join(' · ')),
      el('span', 'proc-time', p.startTime || ''),
      stopBtn
    );
    els.procList.appendChild(row);
  }
}

async function stopOne(pid) {
  const n = await window.dsh.stopDsh([pid]);
  showToast(n > 0 ? t('toastStoppedProc', pid) : t('toastProcGone', pid), n > 0 ? 'warn' : 'muted');
}

async function stopAll() {
  const n = await window.dsh.stopDsh();
  showToast(n > 0 ? t('toastStoppedAll', n) : t('toastNoProcs'), n > 0 ? 'warn' : 'muted');
}

// ---------------- 状态渲染 ----------------
function renderState(s) {
  installedVersion = s.installedVersion;
  latestVersion = s.latestVersion;
  busy = s.busy;
  lastConfig = s.config;

  els.installed.textContent = s.installedVersion || t('unknown');
  els.latest.textContent = s.latestVersion || '…';
  els.lastCheck.textContent = s.lastCheckTime ? `${t('lastChecked', fmtTime(s.lastCheckTime))}` : '';

  els.serverDot.className = 'dot' + (s.serverUp ? ' up' : '');
  els.serverState.textContent = s.serverUp
    ? t('serverUp', s.dshUrl)
    : t('serverDown', s.dshUrl);

  updateProcCount((s.dshProcesses || []).length);
  renderProcesses(s.dshProcesses || []);

  if (s.installedVersion && s.latestVersion) {
    const hasUpdate = compareVersions(s.latestVersion, s.installedVersion) > 0;
    setStatus(hasUpdate ? t('statusUpdate') : t('statusLatest'), hasUpdate ? 'warn' : 'ok');
  } else {
    setStatus(t('statusUnchecked'), 'muted');
  }

  applyTheme(s.config && s.config.theme);
  applyLanguage(s.config);

  if (s.rollbackVersion) {
    rollbackTarget = s.rollbackVersion;
    els.btnRollback.hidden = false;
    els.btnRollback.title = t('rollback', s.rollbackVersion);
    els.btnRollback.textContent = t('rollback', s.rollbackVersion);
  } else {
    rollbackTarget = null;
    els.btnRollback.hidden = true;
  }

  const canStop = s.dshRunning || (s.dshProcesses || []).length > 0;
  const canUpdate = !!(s.installedVersion && s.latestVersion &&
    compareVersions(s.latestVersion, s.installedVersion) > 0);

  els.btnOpen.disabled = !!busy;
  els.btnRestart.disabled = !!busy;
  els.btnCheck.disabled = !!busy;
  els.btnUpdate.disabled = !!busy || !canUpdate;
  els.btnStop.disabled = !!busy || !canStop;

  setButtonLoading(els.btnCheck, busy === 'check', t('checkingNow'), t('btnCheck'));
  setButtonLoading(els.btnUpdate, busy === 'update', t('updatingNow'), t('btnUpdate'));
  setButtonLoading(els.btnInstallPlugin, busy === 'plugin', t('installingNow'), t('btnInstall'));
  els.btnInstallPlugin.disabled = !!busy;

  els.updateProgress.hidden = busy !== 'update';
  if (busy === 'update') els.updateStatus.textContent = t('updateProgressDetail');
}

function compareVersions(a, b) {
  // 与 main.js 保持一致的 semver 预发布比较（rc.10 > rc.9 按数字段比较）。
  const parse = (v) => {
    const s = String(v || '').trim().replace(/^v/, '');
    const [core, ...preParts] = s.split('-');
    const nums = (core || '').split('.').map((n) => parseInt(n, 10) || 0);
    while (nums.length < 3) nums.push(0);
    const pre = preParts.length
      ? preParts.join('-').split('.').map((p) => (/^\d+$/.test(p) ? parseInt(p, 10) : p))
      : null;
    return { nums, pre };
  };
  const cmpPre = (xp, yp) => {
    if (xp === null && yp === null) return 0;
    if (xp === null) return 1;
    if (yp === null) return -1;
    const len = Math.max(xp.length, yp.length);
    for (let i = 0; i < len; i++) {
      const x = xp[i];
      const y = yp[i];
      if (x === undefined) return -1;
      if (y === undefined) return 1;
      const xNum = typeof x === 'number';
      const yNum = typeof y === 'number';
      if (xNum && yNum) {
        if (x !== y) return x > y ? 1 : -1;
      } else if (xNum) {
        return 1;
      } else if (yNum) {
        return -1;
      } else if (x !== y) {
        return x > y ? 1 : -1;
      }
    }
    return 0;
  };
  const x = parse(a);
  const y = parse(b);
  for (let i = 0; i < 3; i++) {
    if (x.nums[i] !== y.nums[i]) return x.nums[i] > y.nums[i] ? 1 : -1;
  }
  return cmpPre(x.pre, y.pre);
}

// ---------------- 用量面板 ----------------
function computeCost(u, cfg) {
  const inCost = (u.uncachedInput / 1e6) * (Number(cfg && cfg.costInput) || 0);
  const cacheCost = (u.cacheRead / 1e6) * (Number(cfg && cfg.costCache) || 0);
  const outCost = (u.output / 1e6) * (Number(cfg && cfg.costOutput) || 0);
  return inCost + cacheCost + outCost;
}

function renderUsage(usage, cfg) {
  const t = usage.totals || {};
  els.statSessions.textContent = String(t.sessions || 0);
  els.statOutput.textContent = fmtTokens(t.output);
  els.statInput.textContent = fmtTokens(t.uncachedInput);
  els.statCache.textContent = fmtTokens(t.cacheRead);
  els.statCost.textContent = fmtCost(computeCost(t, cfg));

  // 项目排行
  const projects = usage.projects || [];
  const maxProj = Math.max(1, ...projects.map((p) => p.output + p.uncachedInput + p.cacheRead));
  els.projectBars.innerHTML = '';
  if (!projects.length) {
    els.projectBars.append(el('div', 'plugin-empty', t('projEmpty')));
  }
  for (const p of projects) {
    const row = el('div', 'project-row');
    const name = el('span', 'project-name', p.name);
    name.title = p.cwd;
    const track = el('div', 'bar-track');
    const fill = el('div', 'bar-fill');
    fill.style.width = Math.max(2, ((p.output + p.uncachedInput + p.cacheRead) / maxProj) * 100) + '%';
    track.append(fill);
    const val = el('span', 'project-val',
      t('projVal', fmtTokens(p.output), fmtTokens(p.output + p.uncachedInput + p.cacheRead), p.sessions));
    row.append(name, track, val);
    els.projectBars.appendChild(row);
  }

  // 每日趋势
  const days = usage.daily || [];
  const maxDay = Math.max(1, ...days.map((d) => d.output + d.uncachedInput + d.cacheRead));
  els.dailyChart.innerHTML = '';
  for (const d of days) {
    const col = el('div', 'day-col');
    const bar = el('div', 'day-bar');
    const segs = [
      ['seg-cache', d.cacheRead],
      ['seg-input', d.uncachedInput],
      ['seg-output', d.output],
    ];
    for (const [cls, v] of segs) {
      const seg = el('div', 'seg ' + cls);
      seg.style.height = (v / maxDay) * 100 + '%';
      bar.appendChild(seg);
    }
    const label = el('div', 'day-label', (d.day || '').slice(5));
    label.title = t('dayTip', d.day, fmtTokens(d.output), fmtTokens(d.uncachedInput), fmtTokens(d.cacheRead));
    col.append(bar, label);
    els.dailyChart.appendChild(col);
  }
}

async function loadUsage() {
  const [usage, state] = await Promise.all([window.dsh.getUsage(), window.dsh.getState()]);
  if (!state.config) return;
  cachedUsage = usage;
  els.priceInput.value = state.config.costInput;
  els.priceCache.value = state.config.costCache;
  els.priceOutput.value = state.config.costOutput;
  renderUsage(usage, state.config);
}

// ---------------- 插件面板 ----------------
function renderPlugins(plugins) {
  els.pluginList.innerHTML = '';
  if (!plugins || !plugins.length) {
    els.pluginList.append(el('div', 'plugin-empty', t('installedEmpty')));
    return;
  }
  for (const p of plugins) {
    const row = el('div', 'plugin-row');
    row.append(el('span', 'plugin-name', p.name), el('span', 'plugin-ver', 'v' + p.version));
    const u = pluginUpdateMap[p.name];
    if (u && u.latest && u.hasUpdate) {
      row.append(el('span', 'plugin-ver-up', '→ v' + u.latest));
      const ub = el('button', 'btn btn-small btn-primary', t('btnUpgrade'));
      ub.addEventListener('click', () => upgradeOne(p.name));
      row.append(ub);
    } else if (u && u.latest) {
      row.append(el('span', 'plugin-ver-up muted', t('pluginUpToDate')));
    }
    const b = el('button', 'btn btn-small btn-danger', t('btnUninstall'));
    b.addEventListener('click', () => removeOne(p.name));
    row.append(b);
    els.pluginList.appendChild(row);
  }
}

async function loadPlugins() {
  const res = await window.dsh.getPlugins();
  renderPlugins(res && res.plugins);
}

async function checkPluginUpdates() {
  showToast(t('pluginCheckingUpdates'), 'info');
  const r = await window.dsh.checkPluginUpdates();
  pluginUpdateMap = {};
  for (const p of (r && r.plugins) || []) pluginUpdateMap[p.name] = p;
  pluginUpdatesChecked = true;
  const hasAny = Object.values(pluginUpdateMap).some((p) => p.hasUpdate);
  if (!hasAny) showToast(t('pluginUpdatesNone'), 'muted');
  loadPlugins();
}

async function upgradeOne(name) {
  const r = await window.dsh.upgradePlugin(name);
  showToast(r && r.ok ? t('toastUpgraded', name) : t('toastUpgradeFailed'), r && r.ok ? 'ok' : 'warn');
  delete pluginUpdateMap[name];
  await loadPlugins();
  checkPluginUpdates();
}

async function removeOne(name) {
  const r = await window.dsh.removePlugin(name);
  showToast(r && r.ok ? t('toastUninstalled', name) : t('toastUninstallFailed'), r && r.ok ? 'warn' : 'muted');
  loadPlugins();
}

// ---------------- Tab 切换 ----------------
function switchTab(name) {
  activeTab = name;
  const tabs = { home: ['tabHome', 'pageHome'], usage: ['tabUsage', 'pageUsage'], plugins: ['tabPlugins', 'pagePlugins'] };
  for (const [k, [tabId, pageId]] of Object.entries(tabs)) {
    const active = k === name;
    $(tabId).classList.toggle('active', active);
    $(pageId).hidden = !active;
  }
  if (name === 'usage') loadUsage();
  if (name === 'plugins') {
    loadPlugins();
    if (!pluginUpdatesChecked) checkPluginUpdates();
  }
}

// ---------------- 配置 ----------------
function loadConfigIntoForm(cfg) {
  els.chkAutoCheck.checked = cfg.autoCheckOnStartup;
  els.chkAutoStart.checked = cfg.autoStartWithWindows;
  els.chkWatchdog.checked = cfg.watchdog !== false;
  els.chkCleanAnalysis.checked = cfg.cleanAnalysisSessions !== false;
  els.chkSilent.checked = !!cfg.minimizeToTrayOnStartup;
  els.selTheme.value = cfg.theme || 'system';
  els.selLang.value = cfg.language || 'system';
  els.txtUrl.value = cfg.dshUrl;
  els.txtInterval.value = cfg.autoCheckIntervalHours;
  els.priceInput.value = cfg.costInput;
  els.priceCache.value = cfg.costCache;
  els.priceOutput.value = cfg.costOutput;
}

function collectConfig() {
  return {
    dshUrl: els.txtUrl.value.trim() || 'http://127.0.0.1:3080',
    autoCheckOnStartup: els.chkAutoCheck.checked,
    autoCheckIntervalHours: Math.max(1, parseInt(els.txtInterval.value, 10) || 6),
    autoStartWithWindows: els.chkAutoStart.checked,
    watchdog: els.chkWatchdog.checked,
    cleanAnalysisSessions: els.chkCleanAnalysis.checked,
    minimizeToTrayOnStartup: els.chkSilent.checked,
    theme: els.selTheme.value || 'system',
    language: els.selLang.value || 'system',
    costInput: Math.max(0, Number(els.priceInput.value) || 0),
    costCache: Math.max(0, Number(els.priceCache.value) || 0),
    costOutput: Math.max(0, Number(els.priceOutput.value) || 0),
  };
}

// ---------------- 主题 ----------------
function applyTheme(theme) {
  const dark = theme === 'dark' ||
    (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
  document.body.classList.toggle('dark', !!dark);
}

// ---------------- 语言 ----------------
function applyLanguage(cfg) {
  const lang = resolveUiLang(cfg);
  if (lang === lastUiLang) return;
  lastUiLang = lang;
  setI18nLang(lang);
  applyI18nStatic();
  // 重建动态区域（渲染层按当前语言重新生成）
  switchTab(activeTab);
  window.dsh.getState().then(renderState);
}

function saveConfig() {
  window.dsh.setConfig(collectConfig());
}

// ---------------- 弹窗 ----------------
function showUpdateModal(latest, text) {
  els.updateVersion.textContent = 'v' + latest;
  els.changelog.textContent = text || t('changelogEmpty');
  els.modalUpdate.hidden = false;
}

function hideUpdateModal() {
  els.modalUpdate.hidden = true;
}

function showAbout(s) {
  els.aboutBody.innerHTML = '';
  const info = s.appInfo || {};
  const rows = [
    [t('aboutAppVersion'), info.appVersion],
    [t('aboutDshVersion'), s.installedVersion],
    [t('aboutElectron'), info.electronVersion],
    [t('aboutNode'), info.nodeVersion],
    [t('aboutChromium'), info.chromeVersion],
  ];
  for (const [k, v] of rows) {
    const r = el('div', 'about-row');
    r.append(el('span', 'about-k', k), el('span', 'about-v', String(v || '—')));
    els.aboutBody.appendChild(r);
  }
  els.modalAbout.hidden = false;
}

function hideAbout() {
  els.modalAbout.hidden = true;
}

// ---------------- 事件绑定 ----------------
els.btnOpen.addEventListener('click', () => window.dsh.openDsh());
els.btnRestart.addEventListener('click', async () => {
  showToast(t('toastRestarting'), 'info');
  await window.dsh.restartDsh();
});
els.btnStop.addEventListener('click', stopAll);
els.btnCheck.addEventListener('click', async () => {
  const res = await window.dsh.checkUpdates(false);
  if (res && res.hasUpdate) showUpdateModal(res.latest, res.changelog);
  else if (res && res.latest) showToast(t('toastLatest'), 'ok');
  else showToast(t('toastCheckFailed'), 'warn');
});
els.btnUpdate.addEventListener('click', async () => {
  const info = await window.dsh.getChangelog();
  if (info && latestVersion) showUpdateModal(latestVersion, info);
  else showToast(t('toastNoUpdate'), 'muted');
});
els.btnUpdateNow.addEventListener('click', async () => {
  hideUpdateModal();
  const r = await window.dsh.update();
  showToast(r && r.ok ? t('toastUpdated') : t('toastUpdateEnded'), r && r.ok ? 'ok' : 'warn');
});
els.btnUpdateLater.addEventListener('click', hideUpdateModal);
els.modalUpdateClose.addEventListener('click', hideUpdateModal);
els.modalUpdate.addEventListener('click', (e) => { if (e.target === els.modalUpdate) hideUpdateModal(); });

els.btnRefresh.addEventListener('click', () => window.dsh.getState().then(renderState));

els.btnConfigDir.addEventListener('click', () => window.dsh.openConfigDir());
els.btnNpmDir.addEventListener('click', () => window.dsh.openNpmDir());
els.btnExportLog.addEventListener('click', async () => {
  const text = [...els.log.children].map((d) => d.textContent).join('\n');
  const res = await window.dsh.exportLog(text);
  if (res && res.ok) showToast(t('toastLogExported'), 'ok');
  else showToast(t('toastExportFailed'), 'muted');
});
els.btnAbout.addEventListener('click', () => window.dsh.getState().then(showAbout));
els.btnAboutClose.addEventListener('click', hideAbout);
els.btnAboutOk.addEventListener('click', hideAbout);
els.modalAbout.addEventListener('click', (e) => { if (e.target === els.modalAbout) hideAbout(); });

els.btnClearLog.addEventListener('click', () => { els.log.innerHTML = ''; });

els.chkAutoCheck.addEventListener('change', saveConfig);
els.chkAutoStart.addEventListener('change', saveConfig);
els.chkWatchdog.addEventListener('change', saveConfig);
els.chkCleanAnalysis.addEventListener('change', saveConfig);
els.chkSilent.addEventListener('change', saveConfig);
els.selTheme.addEventListener('change', () => { saveConfig(); applyTheme(els.selTheme.value); });
els.selLang.addEventListener('change', () => {
  saveConfig();
  const lang = resolveUiLang({ language: els.selLang.value });
  if (lang !== lastUiLang) {
    lastUiLang = '';
    applyLanguage({ language: els.selLang.value });
  }
});
els.txtUrl.addEventListener('change', saveConfig);
els.txtInterval.addEventListener('change', saveConfig);
els.priceInput.addEventListener('change', () => { saveConfig(); refreshCost(); });
els.priceCache.addEventListener('change', () => { saveConfig(); refreshCost(); });
els.priceOutput.addEventListener('change', () => { saveConfig(); refreshCost(); });

els.btnRollback.addEventListener('click', async () => {
  if (!rollbackTarget) return;
  const target = rollbackTarget;
  if (!window.confirm(t('rollbackConfirm', target))) return;
  const r = await window.dsh.rollback();
  showToast(r && r.ok ? t('toastRolledBack', r.newVersion || target) : t('toastRollbackFailed'), r && r.ok ? 'ok' : 'warn');
});

// 系统主题变化（跟随系统时）
window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
  if (lastConfig && (lastConfig.theme || 'system') === 'system') applyTheme('system');
});

els.tabHome.addEventListener('click', () => switchTab('home'));
els.tabUsage.addEventListener('click', () => switchTab('usage'));
els.tabPlugins.addEventListener('click', () => switchTab('plugins'));
els.btnUsageRefresh.addEventListener('click', loadUsage);
els.btnPluginsRefresh.addEventListener('click', loadPlugins);
els.btnPluginsCheck.addEventListener('click', checkPluginUpdates);

// 插件市场：打开独立窗口
els.btnMarketOpen.addEventListener('click', () => window.dsh.openMarket());

// 快速安装（npm）
els.btnInstallPlugin.addEventListener('click', async () => {
  const name = els.pluginName.value.trim();
  if (!name) { showToast(t('toastPluginNameNeeded'), 'warn'); return; }
  const r = await window.dsh.installPlugin(name);
  showToast(r && r.ok ? t('toastInstalled', name) : t('toastInstallFailed'), r && r.ok ? 'ok' : 'warn');
  if (r && r.ok) els.pluginName.value = '';
  loadPlugins();
});

let cachedUsage = null;
async function refreshCost() {
  if (cachedUsage && lastConfig) renderUsage(cachedUsage, collectConfig());
}

// ---------------- 初始化 ----------------
window.dsh.onLog((line) => appendLog(line));
window.dsh.onState((s) => renderState(s));

window.dsh.getRecentLogs().then((lines) => {
  for (const l of lines || []) appendLog(l);
});

window.dsh.getState().then((s) => {
  loadConfigIntoForm(s.config);
  renderState(s);
  switchTab('home');
});
