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
  btnMarketOpen: $('btnMarketOpen'),
};

let busy = null;
let latestVersion = null;
let installedVersion = null;
let lastCount = -1;
let lastConfig = null;
let activeTab = 'home';

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
  els.procCount.textContent = n > 0 ? `检测到 ${n} 个 DSH 进程` : '未检测到 DSH 进程';
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
    const cmd = el('span', 'proc-cmd', p.commandLine || '（未获取到命令行）');
    if (p.commandLine) cmd.title = p.commandLine;
    const stopBtn = el('button', 'btn btn-small btn-danger', '停止');
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
  showToast(n > 0 ? `已停止进程 ${pid}` : `进程 ${pid} 已不存在`, n > 0 ? 'warn' : 'muted');
}

async function stopAll() {
  const n = await window.dsh.stopDsh();
  showToast(n > 0 ? `已停止 ${n} 个 DSH 进程` : '没有运行中的 DSH 进程', n > 0 ? 'warn' : 'muted');
}

// ---------------- 状态渲染 ----------------
function renderState(s) {
  installedVersion = s.installedVersion;
  latestVersion = s.latestVersion;
  busy = s.busy;
  lastConfig = s.config;

  els.installed.textContent = s.installedVersion || '未知';
  els.latest.textContent = s.latestVersion || '…';
  els.lastCheck.textContent = s.lastCheckTime ? `上次检查 ${fmtTime(s.lastCheckTime)}` : '';

  els.serverDot.className = 'dot' + (s.serverUp ? ' up' : '');
  els.serverState.textContent = s.serverUp
    ? `DSH 服务状态：运行中（${s.dshUrl}）`
    : `DSH 服务状态：未运行（${s.dshUrl}）`;

  updateProcCount((s.dshProcesses || []).length);
  renderProcesses(s.dshProcesses || []);

  if (s.installedVersion && s.latestVersion) {
    const hasUpdate = compareVersions(s.latestVersion, s.installedVersion) > 0;
    setStatus(hasUpdate ? '发现新版本' : '已是最新', hasUpdate ? 'warn' : 'ok');
  } else {
    setStatus('未检查', 'muted');
  }

  applyTheme(s.config && s.config.theme);

  if (s.rollbackVersion) {
    els.btnRollback.hidden = false;
    els.btnRollback.title = '回滚到 v' + s.rollbackVersion;
    els.btnRollback.textContent = '回滚 v' + s.rollbackVersion;
  } else {
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

  setButtonLoading(els.btnCheck, busy === 'check', '检查中…', '检查更新');
  setButtonLoading(els.btnUpdate, busy === 'update', '更新中…', '立即更新');
  setButtonLoading(els.btnInstallPlugin, busy === 'plugin', '安装中…', '安装');
  els.btnInstallPlugin.disabled = !!busy;

  els.updateProgress.hidden = busy !== 'update';
  if (busy === 'update') els.updateStatus.textContent = '正在更新 DSH…（详情见日志）';
}

function compareVersions(a, b) {
  const parse = (v) => {
    const s = String(v || '').trim().replace(/^v/, '');
    const [core, ...pre] = s.split('-');
    const nums = (core || '').split('.').map((n) => parseInt(n, 10) || 0);
    while (nums.length < 3) nums.push(0);
    return { nums, pre: pre.join('-') };
  };
  const x = parse(a);
  const y = parse(b);
  for (let i = 0; i < 3; i++) {
    if (x.nums[i] !== y.nums[i]) return x.nums[i] > y.nums[i] ? 1 : -1;
  }
  if (!x.pre && !y.pre) return 0;
  if (!x.pre) return 1;
  if (!y.pre) return -1;
  return x.pre === y.pre ? 0 : (x.pre > y.pre ? 1 : -1);
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
    els.projectBars.append(el('div', 'plugin-empty', '暂无会话数据'));
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
      `输出 ${fmtTokens(p.output)} · 共 ${fmtTokens(p.output + p.uncachedInput + p.cacheRead)} · ${p.sessions} 会话`);
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
    label.title = `${d.day}  输出 ${fmtTokens(d.output)} · 输入 ${fmtTokens(d.uncachedInput)} · 缓存 ${fmtTokens(d.cacheRead)}`;
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
    els.pluginList.append(el('div', 'plugin-empty', '暂无已安装插件'));
    return;
  }
  for (const p of plugins) {
    const row = el('div', 'plugin-row');
    row.append(el('span', 'plugin-name', p.name), el('span', 'plugin-ver', 'v' + p.version));
    const b = el('button', 'btn btn-small btn-danger', '卸载');
    b.addEventListener('click', () => removeOne(p.name));
    row.append(b);
    els.pluginList.appendChild(row);
  }
}

async function loadPlugins() {
  const res = await window.dsh.getPlugins();
  renderPlugins(res && res.plugins);
}

async function removeOne(name) {
  const r = await window.dsh.removePlugin(name);
  showToast(r && r.ok ? `已卸载 ${name}，重启 DSH 生效` : '卸载失败，详见日志', r && r.ok ? 'warn' : 'muted');
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
  if (name === 'plugins') loadPlugins();
}

// ---------------- 配置 ----------------
function loadConfigIntoForm(cfg) {
  els.chkAutoCheck.checked = cfg.autoCheckOnStartup;
  els.chkAutoStart.checked = cfg.autoStartWithWindows;
  els.chkWatchdog.checked = cfg.watchdog !== false;
  els.chkCleanAnalysis.checked = cfg.cleanAnalysisSessions !== false;
  els.chkSilent.checked = !!cfg.minimizeToTrayOnStartup;
  els.selTheme.value = cfg.theme || 'system';
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

function saveConfig() {
  window.dsh.setConfig(collectConfig());
}

// ---------------- 弹窗 ----------------
function showUpdateModal(latest, text) {
  els.updateVersion.textContent = 'v' + latest;
  els.changelog.textContent = text || '（未获取到更新说明）';
  els.modalUpdate.hidden = false;
}

function hideUpdateModal() {
  els.modalUpdate.hidden = true;
}

function showAbout(s) {
  els.aboutBody.innerHTML = '';
  const info = s.appInfo || {};
  const rows = [
    ['管理器版本', info.appVersion],
    ['DSH 版本', s.installedVersion],
    ['Electron', info.electronVersion],
    ['Node.js', info.nodeVersion],
    ['Chromium', info.chromeVersion],
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
  showToast('正在重启 DSH…', 'info');
  await window.dsh.restartDsh();
});
els.btnStop.addEventListener('click', stopAll);
els.btnCheck.addEventListener('click', async () => {
  const res = await window.dsh.checkUpdates(false);
  if (res && res.hasUpdate) showUpdateModal(res.latest, res.changelog);
  else if (res && res.latest) showToast('已是最新版本', 'ok');
  else showToast('检查失败，详见日志', 'warn');
});
els.btnUpdate.addEventListener('click', async () => {
  const info = await window.dsh.getChangelog();
  if (info && latestVersion) showUpdateModal(latestVersion, info);
  else showToast('暂无可用更新', 'muted');
});
els.btnUpdateNow.addEventListener('click', async () => {
  hideUpdateModal();
  const r = await window.dsh.update();
  showToast(r && r.ok ? '更新完成' : '更新结束，详见日志', r && r.ok ? 'ok' : 'warn');
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
  if (res && res.ok) showToast('日志已导出', 'ok');
  else showToast('导出已取消或失败', 'muted');
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
els.txtUrl.addEventListener('change', saveConfig);
els.txtInterval.addEventListener('change', saveConfig);
els.priceInput.addEventListener('change', () => { saveConfig(); refreshCost(); });
els.priceCache.addEventListener('change', () => { saveConfig(); refreshCost(); });
els.priceOutput.addEventListener('change', () => { saveConfig(); refreshCost(); });

els.btnRollback.addEventListener('click', async () => {
  const target = els.btnRollback.title.replace('回滚到 v', '');
  if (!window.confirm(`确定回滚到 v${target}？\n将执行 npm install -g @deepseek-ai/dsh@${target}`)) return;
  const r = await window.dsh.rollback();
  showToast(r && r.ok ? `已回滚到 ${r.newVersion || target}` : '回滚失败，详见日志', r && r.ok ? 'ok' : 'warn');
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

// 插件市场：打开独立窗口
els.btnMarketOpen.addEventListener('click', () => window.dsh.openMarket());

// 快速安装（npm）
els.btnInstallPlugin.addEventListener('click', async () => {
  const name = els.pluginName.value.trim();
  if (!name) { showToast('请输入插件包名', 'warn'); return; }
  const r = await window.dsh.installPlugin(name);
  showToast(r && r.ok ? `已安装 ${name}，重启 DSH 生效` : '安装失败，详见日志', r && r.ok ? 'ok' : 'warn');
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
