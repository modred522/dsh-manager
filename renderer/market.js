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

// ---------------- busy 状态 ----------------
function applyBusy(b) {
  busy = b;
  const analyzing = busy === 'analyze';
  const installing = busy === 'plugin';
  els.btnMarketSearch.disabled = !!busy;
  els.btnPluginInstall.disabled = !!busy;
  els.btnPluginAnalyze.disabled = !!busy;
  els.btnPluginStop.hidden = !analyzing;
  els.btnPluginInstall.textContent = installing ? '安装中…' : '安装';
  els.btnPluginAnalyze.textContent = analyzing ? '分析中…' : (currentDetail ? '重新分析' : '分析');
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
  els.marketQuery.placeholder = source === 'npm' ? '搜索 npm dsh 插件（留空看热门）' : '搜索 GitHub 仓库（留空看热门）';
}

async function searchMarket() {
  showToast(marketSource === 'npm' ? '正在搜索 npm…' : '正在搜索 GitHub…', 'info');
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
  setMarketFooter(reset ? '正在搜索…' : '正在加载更多…', true);
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
    setMarketFooter('加载失败（网络异常），点击重试', false);
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
    showToast('部分搜索源触发限流（GitHub 匿名接口 10 次/分钟），稍后重试可查看更多', 'warn');
  }
  if (reset && !(r.items || []).length && marketRateLimited) {
    setMarketFooter('加载失败（网络异常或触发限流），点击重试', false);
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
    text = '正在加载…';
  } else if (marketHasMore) {
    text = '继续向下滚动加载更多…';
  } else if (n === 0) {
    text = '';
  } else {
    text = `已加载全部 ${n} 个插件`;
    if (marketRateLimited) text += '（部分源触发限流，稍后重试可查看更多）';
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
  if (!results || !results.length) {
    // 只有整页都空时才显示空态（分页追加时忽略）
    if (els.marketList.childElementCount === 0) {
      els.marketList.append(el('div', 'plugin-empty', '没有找到相关插件'));
    }
    return;
  }
  for (const p of results) {
    const card = el('div', 'market-card');
    const head = el('div', 'market-head');
    head.append(el('span', 'market-name', marketSource === 'npm' ? p.name : p.repo));
    if (marketSource === 'npm') {
      head.append(el('span', 'badge ' + (p.scope === 'official' ? 'badge-official' : 'badge-community'), p.scope === 'official' ? '官方' : '社区'));
    } else {
      head.append(el('span', 'badge badge-community', 'GitHub'));
    }
    card.append(head);
    const desc = el('div', 'market-desc', p.description || '（无描述）');
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
    const btnDetail = el('button', 'btn btn-small', '详情');
    btnDetail.addEventListener('click', () => openDetail(marketSource, marketSource === 'npm' ? p.name : p.repo, false));
    const btnAnalyze = el('button', 'btn btn-small', '分析');
    btnAnalyze.addEventListener('click', () => openDetail(marketSource, marketSource === 'npm' ? p.name : p.repo, true));
    const btnInstall = el('button', 'btn btn-small btn-primary', '安装');
    btnInstall.addEventListener('click', () => installFromMarket(marketSource, marketSource === 'npm' ? p.name : p.repo));
    actions.append(btnDetail, btnAnalyze, btnInstall);
    card.append(actions);
    els.marketList.appendChild(card);
  }
}

async function installFromMarket(source, ref) {
  if (source === 'npm') {
    const r = await window.dsh.installPlugin(ref);
    showToast(r && r.ok ? `已安装 ${ref}，重启 DSH 生效` : '安装失败，详见日志', r && r.ok ? 'ok' : 'warn');
  } else {
    const ok = window.confirm(
      `将从 GitHub 安装 ${ref}。\n\n该插件带安装期构建脚本（prepare），安装即允许其在本机执行代码——这是供应链攻击的常见入口。\n\n请确认你信任该仓库后继续。`
    );
    if (!ok) return;
    const parts = ref.split('/');
    const r = await window.dsh.installGithubPlugin(parts[0], parts[1]);
    showToast(r && r.ok ? `已安装 ${ref}，重启 DSH 生效` : '安装失败，详见日志', r && r.ok ? 'ok' : 'warn');
  }
}

// ---------------- 插件详情视图 ----------------
async function openDetail(source, ref, startAnalysis) {
  currentDetail = { source, ref };
  els.pluginTitle.textContent = ref;
  els.pluginTitle.title = ref;
  els.pluginBadges.innerHTML = '';
  els.pluginStats.innerHTML = '';
  els.pluginReadme.textContent = '加载中…';
  els.analysisScore.innerHTML = '';
  els.analysisLog.innerHTML = '';
  els.btnPluginAnalyze.textContent = '分析';
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
    els.btnPluginAnalyze.textContent = '重新分析';
  }

  if (startAnalysis) startAnalysisFlow(false);
}

function backToMarket() {
  currentDetail = null;
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
  if (!info) { els.pluginReadme.textContent = '获取详情失败（网络异常或包不存在）。'; return; }
  els.pluginTitle.textContent = `${info.name}  v${info.version}`;
  els.pluginTitle.title = `${info.name}  v${info.version}`;
  addBadge('npm');
  addBadge(info.license || '无许可证');
  addStats([
    ['周下载', info.downloads != null ? fmtTokens(info.downloads) : '—'],
    ['Star', info.repo ? String(info.repo.stars) : '—'],
    ['Issues', info.repo ? String(info.repo.openIssues) : '—'],
    ['创建', info.created || '—'],
    ['更新', info.modified || '—'],
  ]);
  const gh = (info.repo && info.repo.fullName)
    ? `https://github.com/${info.repo.fullName}`
    : githubUrlFrom(info.repository) || githubUrlFrom(info.homepage);
  if (gh) addPluginLink('GitHub', gh);
  els.pluginReadme.textContent = info.readme || '（无 README）';
}

function renderDetailGithub(info) {
  if (!info || !info.stats) { els.pluginReadme.textContent = '获取详情失败（可能被 GitHub 限流或仓库不存在）。'; return; }
  const s = info.stats;
  els.pluginTitle.textContent = info.repo;
  els.pluginTitle.title = info.repo;
  addBadge('GitHub');
  addBadge(s.license || '无许可证');
  addStats([
    ['Star', String(s.stars || 0)],
    ['Fork', String(s.forks || 0)],
    ['Issues', String(s.openIssues || 0)],
    ['创建', s.created || '—'],
    ['推送', s.pushed || '—'],
  ]);
  addPluginLink('GitHub', `https://github.com/${info.repo}`);
  els.pluginReadme.textContent = s.readme || '（无 README）';
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
  els.analysisLog.innerHTML = '';
  els.btnPluginAnalyze.disabled = true;
  els.btnPluginInstall.disabled = true;
  els.btnPluginStop.hidden = false;
  els.btnPluginAnalyze.textContent = '分析中…';
  await window.dsh.pluginAnalyze(currentDetail.source, currentDetail.ref, !!force);
  els.btnPluginStop.hidden = true;
  els.btnPluginAnalyze.disabled = false;
  els.btnPluginInstall.disabled = false;
  els.btnPluginAnalyze.textContent = '重新分析';
}

function renderAnalysisResult(r) {
  els.analysisScore.innerHTML = '';
  const wrap = el('div', 'score-card');
  const head = el('div', 'score-head');
  head.append(
    el('div', 'score-big', r.score != null ? String(r.score) : '?'),
    el('span', 'verdict-badge verdict-' +
      (r.verdict === '真实有用' ? 'good' : r.verdict === '徒有其表' ? 'bad' : r.verdict === '一般' ? 'mid' : 'unknown'),
      r.verdict || '未知')
  );
  wrap.append(head);
  wrap.append(el('div', 'score-summary', r.summary || ''));
  const lists = [['优点', r.pros], ['缺点', r.cons], ['风险', r.risks]];
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
els.btnPluginAnalyze.addEventListener('click', () => startAnalysisFlow(els.btnPluginAnalyze.textContent === '重新分析'));
els.btnPluginStop.addEventListener('click', () => window.dsh.pluginAnalyzeStop());
els.btnPluginInstall.addEventListener('click', () => {
  if (currentDetail) installFromMarket(currentDetail.source, currentDetail.ref);
});

window.dsh.onAnalyzeLog((line) => {
  const div = el('div', 'log-line', line);
  els.analysisLog.appendChild(div);
  while (els.analysisLog.childElementCount > 500) els.analysisLog.removeChild(els.analysisLog.firstChild);
  els.analysisLog.scrollTop = els.analysisLog.scrollHeight;
});
window.dsh.onAnalyzeDone((r) => {
  renderAnalysisResult(r);
  els.btnPluginStop.hidden = true;
  els.btnPluginAnalyze.disabled = false;
  els.btnPluginInstall.disabled = false;
  els.btnPluginAnalyze.textContent = '重新分析';
});

window.dsh.onState((s) => {
  applyBusy(s.busy);
  if (s.config) applyTheme(s.config.theme);
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
  if (s.config) applyTheme(s.config.theme);
});
searchMarket();
els.marketQuery.focus();
