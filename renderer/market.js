'use strict';

const $ = (id) => document.getElementById(id);

const els = {
  // 市场列表视图
  viewMarket: $('viewMarket'),
  srcNpm: $('srcNpm'),
  srcGithub: $('srcGithub'),
  marketQuery: $('marketQuery'),
  btnMarketSearch: $('btnMarketSearch'),
  marketList: $('marketList'),
  marketFooter: $('marketFooter'),
  // 详情视图
  viewDetail: $('viewDetail'),
  btnBack: $('btnBack'),
  pluginTitle: $('pluginTitle'),
  pluginBadges: $('pluginBadges'),
  pluginStats: $('pluginStats'),
  pluginReadme: $('pluginReadme'),
  analysisScore: $('analysisScore'),
  analysisLog: $('analysisLog'),
  btnPluginInstall: $('btnPluginInstall'),
  btnPluginAnalyze: $('btnPluginAnalyze'),
  btnPluginStop: $('btnPluginStop'),
  // 分栏
  detailSplit: $('detailSplit'),
  paneReadme: $('paneReadme'),
  paneAnalysis: $('paneAnalysis'),
  paneScore: $('paneScore'),
  splitV: $('splitV'),
  splitH: $('splitH'),
  toasts: $('toasts'),
};

let marketSource = 'npm';
let busy = null;
let currentDetail = null; // { source, ref }
let analyzedOnce = false;
let logPlaceholder = true;
let lastMarketResults = [];
let lastUiLang = '';

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
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
  setMarketSource(marketSource);
  renderMarket(lastMarketResults);
  applyBusy(busy);
  if (currentDetail) {
    if (!els.analysisScore.childElementCount) setScorePlaceholder();
    if (logPlaceholder) setLogPlaceholder();
    els.btnPluginAnalyze.textContent = analyzedOnce ? t('reanalyze') : t('btnAnalyze');
  }
}

function setScorePlaceholder() {
  els.analysisScore.innerHTML = '';
  els.analysisScore.classList.add('pane-placeholder');
  els.analysisScore.textContent = t('scoreEmpty');
}

function setLogPlaceholder() {
  els.analysisLog.innerHTML = '';
  els.analysisLog.classList.add('pane-placeholder');
  els.analysisLog.textContent = t('logEmptyHint');
  logPlaceholder = true;
}

// ---------------- busy 状态 ----------------
function applyBusy(b) {
  busy = b;
  const analyzing = busy === 'analyze';
  const installing = busy === 'plugin';
  els.btnMarketSearch.disabled = !!busy;
  els.btnPluginInstall.disabled = !!busy;
  els.btnPluginAnalyze.disabled = !!busy;
  els.btnPluginStop.hidden = !analyzing;
  els.btnPluginInstall.textContent = installing ? t('installingNow') : t('btnInstall');
  els.btnPluginAnalyze.textContent = analyzing ? t('analyzing')
    : (currentDetail ? (analyzedOnce ? t('reanalyze') : t('btnAnalyze')) : t('btnAnalyze'));
}

// ---------------- 可拖拽分栏 ----------------
const SPLIT_DEFAULT_V = 0.46; // README 左栏默认宽度比例
const SPLIT_DEFAULT_H = 0.55; // 评分卡默认高度比例（相对右栏）

function loadSplitRatio(key, def) {
  const v = parseFloat(localStorage.getItem(key));
  return isFinite(v) && v > 0.05 && v < 0.95 ? v : def;
}

let splitV = loadSplitRatio('dshMarket.splitV', SPLIT_DEFAULT_V);
let splitH = loadSplitRatio('dshMarket.splitH', SPLIT_DEFAULT_H);

function applySplitSizes() {
  els.paneReadme.style.flex = '0 0 ' + (splitV * 100).toFixed(2) + '%';
  els.paneScore.style.flex = '0 0 ' + (splitH * 100).toFixed(2) + '%';
}

function bindSplitter(handle, container, isVertical, onChange, onCommit) {
  let dragging = false;

  handle.addEventListener('pointerdown', (e) => {
    if (e.button !== 0) return;
    dragging = true;
    handle.setPointerCapture(e.pointerId);
    document.body.style.cursor = isVertical ? 'col-resize' : 'row-resize';
    e.preventDefault();
  });

  handle.addEventListener('pointermove', (e) => {
    if (!dragging) return;
    const rect = container.getBoundingClientRect();
    const r = isVertical ? (e.clientX - rect.left) / rect.width : (e.clientY - rect.top) / rect.height;
    const clamped = Math.min(0.85, Math.max(0.15, r));
    if (isVertical) splitV = clamped;
    else splitH = clamped;
    onChange();
  });

  const end = () => {
    if (!dragging) return;
    dragging = false;
    document.body.style.cursor = '';
    onCommit(isVertical ? splitV : splitH);
  };
  handle.addEventListener('pointerup', end);
  handle.addEventListener('pointercancel', end);

  handle.addEventListener('dblclick', () => {
    const def = isVertical ? SPLIT_DEFAULT_V : SPLIT_DEFAULT_H;
    if (isVertical) splitV = def;
    else splitH = def;
    onChange();
    onCommit(def);
  });
}

// ---------------- 插件市场 ----------------
function setMarketSource(source) {
  marketSource = source;
  els.srcNpm.classList.toggle('src-active', source === 'npm');
  els.srcGithub.classList.toggle('src-active', source === 'github');
  els.marketQuery.placeholder = source === 'npm' ? t('searchNpmPh') : t('searchGhPh');
}

async function searchMarket() {
  showToast(marketSource === 'npm' ? t('toastSearchingNpm') : t('toastSearchingGh'), 'info');
  await fetchMarketPage(true);
}

// ---------------- 分页加载（滑到底部继续加载） ----------------
let marketLoading = false;
let marketHasMore = false;
let marketRateLimited = false;
let marketReq = 0;

async function fetchMarketPage(reset) {
  const reqId = ++marketReq;
  const query = els.marketQuery.value.trim();
  marketLoading = true;
  setMarketFooter(reset ? t('footerSearching') : t('footerLoadingMore'), true);
  let r;
  try {
    r = await window.dsh.marketSearch(marketSource, query, reset);
  } catch {
    r = null;
  }
  if (reqId !== marketReq) return; // 期间用户又发起了新搜索/切源，丢弃过期响应
  marketLoading = false;
  if (!r) {
    marketHasMore = false;
    setMarketFooter(t('footerFailNet'), false);
    els.marketFooter.classList.add('clickable');
    return;
  }
  if (reset) {
    els.marketList.innerHTML = '';
    marketRateLimited = false;
  }
  renderMarket(r.items || []);
  marketHasMore = !!r.hasMore;
  marketRateLimited = marketRateLimited || !!r.rateLimited;
  updateMarketFooter();
  if (reset && r.rateLimited && (r.items || []).length) {
    showToast(t('toastRateLimited'), 'warn');
  }
  if (reset && !(r.items || []).length && marketRateLimited) {
    setMarketFooter(t('footerFailLimit'), false);
    els.marketFooter.classList.add('clickable');
  }
}

function loadMore() {
  if (marketLoading || !marketHasMore) return;
  fetchMarketPage(false);
}

function setMarketFooter(text, loading) {
  els.marketFooter.textContent = text;
  els.marketFooter.classList.toggle('loading', !!loading);
}

function updateMarketFooter() {
  const n = els.marketList.querySelectorAll('.market-card').length;
  let text;
  if (marketLoading) {
    text = t('footerLoading');
  } else if (marketHasMore) {
    text = t('footerScrollMore');
  } else if (n === 0) {
    text = '';
  } else {
    text = t('footerAllLoaded', n);
    if (marketRateLimited) text += t('footerRateLimited');
  }
  setMarketFooter(text, marketLoading);
  els.marketFooter.classList.toggle('clickable', !marketLoading && marketHasMore);
}

function openExternal(url) {
  window.dsh.openExternal(url).catch(() => {});
}

function makeLink(url, cls, text) {
  const a = el('a', cls, text);
  a.href = url;
  a.title = url;
  a.addEventListener('click', (e) => {
    e.preventDefault();
    openExternal(url);
  });
  return a;
}

function renderMarket(results) {
  lastMarketResults = results || [];
  if (!results || !results.length) {
    // 只有整页都空时才显示空态（分页追加时忽略）
    if (els.marketList.childElementCount === 0) {
      els.marketList.append(el('div', 'plugin-empty', t('marketEmpty')));
    }
    return;
  }
  for (const p of results) {
    const card = el('div', 'market-card');
    const head = el('div', 'market-head');
    head.append(el('span', 'market-name', marketSource === 'npm' ? p.name : p.repo));
    if (marketSource === 'npm') {
      head.append(el('span', 'badge ' + (p.scope === 'official' ? 'badge-official' : 'badge-community'), p.scope === 'official' ? t('badgeOfficial') : t('badgeCommunity')));
    } else {
      head.append(el('span', 'badge badge-community', t('badgeGithub')));
    }
    card.append(head);
    const desc = el('div', 'market-desc', p.description || t('noDescription'));
    desc.title = p.description || '';
    card.append(desc);
    const meta = el('div', 'market-meta');
    if (marketSource === 'npm') {
      meta.append(el('span', '', 'v' + p.version), el('span', '', p.date || ''));
    } else {
      meta.append(el('span', '', '★ ' + p.stars), el('span', '', p.language || ''), el('span', '', p.updated || ''));
    }
    card.append(meta);

    // GitHub 页面链接（npm 源无仓库信息时回退到 npm 页面）
    let url;
    if (marketSource === 'npm') {
      url = p.repoUrl || `https://www.npmjs.com/package/${p.name}`;
    } else {
      url = p.url || `https://github.com/${p.repo}`;
    }
    card.append(makeLink(url, 'market-link', url));

    const actions = el('div', 'market-actions');
    const btnDetail = el('button', 'btn btn-small', t('btnDetail'));
    btnDetail.addEventListener('click', () => openDetail(marketSource, marketSource === 'npm' ? p.name : p.repo, false));
    const btnAnalyze = el('button', 'btn btn-small', t('btnAnalyze'));
    btnAnalyze.addEventListener('click', () => openDetail(marketSource, marketSource === 'npm' ? p.name : p.repo, true));
    const btnInstall = el('button', 'btn btn-small btn-primary', t('btnInstall'));
    btnInstall.addEventListener('click', () => installFromMarket(marketSource, marketSource === 'npm' ? p.name : p.repo));
    actions.append(btnDetail, btnAnalyze, btnInstall);
    card.append(actions);
    els.marketList.appendChild(card);
  }
}

async function installFromMarket(source, ref) {
  if (source === 'npm') {
    const r = await window.dsh.installPlugin(ref);
    showToast(r && r.ok ? t('toastInstalled', ref) : t('toastInstallFailed'), r && r.ok ? 'ok' : 'warn');
  } else {
    const ok = window.confirm(t('ghInstallConfirm', ref));
    if (!ok) return;
    const parts = ref.split('/');
    const r = await window.dsh.installGithubPlugin(parts[0], parts[1]);
    showToast(r && r.ok ? t('toastInstalled', ref) : t('toastInstallFailed'), r && r.ok ? 'ok' : 'warn');
  }
}

// ---------------- 插件详情视图 ----------------
async function openDetail(source, ref, startAnalysis) {
  currentDetail = { source, ref };
  analyzedOnce = false;
  els.pluginTitle.textContent = ref;
  els.pluginTitle.title = ref;
  els.pluginBadges.innerHTML = '';
  els.pluginStats.innerHTML = '';
  els.pluginReadme.textContent = t('loadingDetail');
  setScorePlaceholder();
  setLogPlaceholder();
  els.btnPluginAnalyze.textContent = t('btnAnalyze');
  els.btnPluginStop.hidden = true;
  els.viewMarket.hidden = true;
  els.viewDetail.hidden = false;

  if (source === 'npm') {
    renderDetailNpm(await window.dsh.pluginInfo(ref));
  } else {
    const parts = ref.split('/');
    renderDetailGithub(await window.dsh.githubPluginInfo(parts[0], parts[1]));
  }

  const hist = await window.dsh.analysisHistory(source, ref);
  if (hist && hist.result && hist.result.score != null) {
    renderAnalysisResult(hist.result);
    analyzedOnce = true;
    els.btnPluginAnalyze.textContent = t('reanalyze');
  }

  if (startAnalysis) startAnalysisFlow(false);
}

function backToMarket() {
  currentDetail = null;
  analyzedOnce = false;
  els.viewDetail.hidden = true;
  els.viewMarket.hidden = false;
  els.marketQuery.focus();
}

function githubUrlFrom(raw) {
  const s = String(raw || '');
  const m = s.match(/github\.com\/([^/]+)\/([^/?#.]+)/);
  return m ? `https://github.com/${m[1]}/${m[2]}` : '';
}

function renderDetailNpm(info) {
  if (!info) { els.pluginReadme.textContent = t('detailFailNpm'); return; }
  els.pluginTitle.textContent = `${info.name}  v${info.version}`;
  els.pluginTitle.title = `${info.name}  v${info.version}`;
  addBadge(t('badgeNpm'));
  addBadge(info.license || t('badgeNoLicense'));
  addStats([
    [t('statDownloads'), info.downloads != null ? fmtTokens(info.downloads) : '—'],
    [t('statStars'), info.repo ? String(info.repo.stars) : '—'],
    [t('statIssues'), info.repo ? String(info.repo.openIssues) : '—'],
    [t('statCreated'), info.created || '—'],
    [t('statUpdated'), info.modified || '—'],
  ]);
  const gh = (info.repo && info.repo.fullName)
    ? `https://github.com/${info.repo.fullName}`
    : githubUrlFrom(info.repository) || githubUrlFrom(info.homepage);
  if (gh) addPluginLink('GitHub', gh);
  els.pluginReadme.textContent = info.readme || t('noReadme');
}

function renderDetailGithub(info) {
  if (!info || !info.stats) { els.pluginReadme.textContent = t('detailFailGh'); return; }
  const s = info.stats;
  els.pluginTitle.textContent = info.repo;
  els.pluginTitle.title = info.repo;
  addBadge(t('badgeGithub'));
  addBadge(s.license || t('badgeNoLicense'));
  addStats([
    [t('statStars'), String(s.stars || 0)],
    [t('statForks'), String(s.forks || 0)],
    [t('statIssues'), String(s.openIssues || 0)],
    [t('statCreated'), s.created || '—'],
    [t('statPushed'), s.pushed || '—'],
  ]);
  addPluginLink('GitHub', `https://github.com/${info.repo}`);
  els.pluginReadme.textContent = s.readme || t('noReadme');
}

function addBadge(text) {
  els.pluginBadges.appendChild(el('span', 'badge badge-community', text));
}

function addPluginLink(text, url) {
  els.pluginBadges.appendChild(makeLink(url, 'badge badge-community badge-link', text));
}

function addStats(pairs) {
  for (const [k, v] of pairs) {
    const cell = el('div', 'stat-cell');
    cell.append(el('span', 'stat-cell-label', k), el('span', 'stat-cell-value', v));
    els.pluginStats.appendChild(cell);
  }
}

async function startAnalysisFlow(force) {
  if (!currentDetail) return;
  els.analysisScore.innerHTML = '';
  els.analysisScore.classList.remove('pane-placeholder');
  els.analysisLog.innerHTML = '';
  els.analysisLog.classList.remove('pane-placeholder');
  logPlaceholder = false;
  els.btnPluginAnalyze.disabled = true;
  els.btnPluginInstall.disabled = true;
  els.btnPluginStop.hidden = false;
  els.btnPluginAnalyze.textContent = t('analyzing');
  await window.dsh.pluginAnalyze(currentDetail.source, currentDetail.ref, !!force);
  analyzedOnce = true;
  els.btnPluginStop.hidden = true;
  els.btnPluginAnalyze.disabled = false;
  els.btnPluginInstall.disabled = false;
  els.btnPluginAnalyze.textContent = t('reanalyze');
}

function renderAnalysisResult(r) {
  els.analysisScore.innerHTML = '';
  els.analysisScore.classList.remove('pane-placeholder');
  const wrap = el('div', 'score-card');
  const head = el('div', 'score-head');
  head.append(
    el('div', 'score-big', r.score != null ? String(r.score) : '?'),
    el('span', 'verdict-badge verdict-' +
      (r.verdict === '真实有用' ? 'good' : r.verdict === '徒有其表' ? 'bad' : r.verdict === '一般' ? 'mid' : 'unknown'),
      r.verdict || t('unknown'))
  );
  wrap.append(head);
  wrap.append(el('div', 'score-summary', r.summary || ''));
  const lists = [[t('scorePros'), r.pros], [t('scoreCons'), r.cons], [t('scoreRisks'), r.risks]];
  for (const [title, items] of lists) {
    if (!items || !items.length) continue;
    const sec = el('div', 'score-section');
    sec.append(el('div', 'score-section-title', title));
    for (const it of items) sec.append(el('div', 'score-item', it));
    wrap.append(sec);
  }
  els.analysisScore.appendChild(wrap);
}

// ---------------- 事件绑定 ----------------
els.srcNpm.addEventListener('click', () => { setMarketSource('npm'); searchMarket(); });
els.srcGithub.addEventListener('click', () => { setMarketSource('github'); searchMarket(); });
els.btnMarketSearch.addEventListener('click', searchMarket);
els.marketQuery.addEventListener('keydown', (e) => { if (e.key === 'Enter') searchMarket(); });

// 滑到底部自动加载下一页；底部状态条也可点击触发
els.marketList.addEventListener('scroll', () => {
  if (marketLoading || !marketHasMore) return;
  const list = els.marketList;
  if (list.scrollTop + list.clientHeight >= list.scrollHeight - 120) loadMore();
});
els.marketFooter.addEventListener('click', () => {
  if (marketLoading) return;
  if (marketHasMore) loadMore();
  else if (marketRateLimited) searchMarket(); // 失败/限流状态下点击重试
});

els.btnBack.addEventListener('click', backToMarket);
els.btnPluginAnalyze.addEventListener('click', () => startAnalysisFlow(analyzedOnce));
els.btnPluginStop.addEventListener('click', () => window.dsh.pluginAnalyzeStop());
els.btnPluginInstall.addEventListener('click', () => {
  if (currentDetail) installFromMarket(currentDetail.source, currentDetail.ref);
});

window.dsh.onAnalyzeLog((line) => {
  if (logPlaceholder) {
    els.analysisLog.innerHTML = '';
    els.analysisLog.classList.remove('pane-placeholder');
    logPlaceholder = false;
  }
  const div = el('div', 'log-line', line);
  els.analysisLog.appendChild(div);
  while (els.analysisLog.childElementCount > 500) els.analysisLog.removeChild(els.analysisLog.firstChild);
  els.analysisLog.scrollTop = els.analysisLog.scrollHeight;
});
window.dsh.onAnalyzeDone((r) => {
  analyzedOnce = true;
  renderAnalysisResult(r);
  els.btnPluginStop.hidden = true;
  els.btnPluginAnalyze.disabled = false;
  els.btnPluginInstall.disabled = false;
  els.btnPluginAnalyze.textContent = t('reanalyze');
});

window.dsh.onState((s) => {
  applyBusy(s.busy);
  if (s.config) {
    applyTheme(s.config.theme);
    applyLanguage(s.config);
  }
});

// ---------------- 初始化 ----------------
setMarketSource('npm');
applySplitSizes();
bindSplitter(els.splitV, els.detailSplit, true,
  () => applySplitSizes(),
  (v) => localStorage.setItem('dshMarket.splitV', String(v)));
bindSplitter(els.splitH, els.paneAnalysis, false,
  () => applySplitSizes(),
  (v) => localStorage.setItem('dshMarket.splitH', String(v)));

window.dsh.getState().then((s) => {
  applyBusy(s.busy);
  if (s.config) {
    applyTheme(s.config.theme);
    applyLanguage(s.config);
  }
});
searchMarket();
els.marketQuery.focus();
