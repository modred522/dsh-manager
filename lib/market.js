'use strict';
// 插件市场（npm / GitHub 双源）搜索与详情：只读、纯网络，无 Electron 依赖。
const { normalizeGithubUrl, npmItem, githubItem } = require('./pure');

const NPM_PAGE_SIZE = 25;   // npm 接口每页条数
const GH_PAGE_SIZE = 20;    // GitHub 接口每页条数
const MARKET_BATCH = 20;    // 每次返回给渲染层的条数
const NPM_MAX_FROM = 500;   // npm 搜索偏移上限（防御性终止）
const GH_MAX_PAGE = 50;     // GitHub 搜索最多 1000 条（20 条/页 × 50 页）

// 排序说明：GitHub 固定 sort=stars（星标降序）；
// npm 走 registry 默认相关度排序（内部综合 质量/维护/流行度 打分，非纯下载量）。
function npmQueries(q) {
  const list = [];
  if (q) list.push(q);
  list.push('keywords:dsh-plugin', 'keywords:dsh');
  return list;
}

function githubQueries(q) {
  const list = [];
  if (q) list.push(q);
  list.push('dsh plugin', 'deepseek-harness plugin', 'topic:dsh');
  return list;
}

async function fetchNpmPage(query, from) {
  const url = `https://registry.npmjs.org/-/v1/search?text=${encodeURIComponent(query)}&size=${NPM_PAGE_SIZE}&from=${from}`;
  try {
    const res = await fetch(url, { headers: { 'User-Agent': 'dsh-manager' } });
    if (!res.ok) return null;
    const j = await res.json();
    return (j.objects || []).map((o) => o.package).filter(Boolean);
  } catch {
    return null;
  }
}

async function fetchGithubPage(query, page) {
  const url = `https://api.github.com/search/repositories?q=${encodeURIComponent(query)}&per_page=${GH_PAGE_SIZE}&page=${page}&sort=stars`;
  try {
    const res = await fetch(url, { headers: { 'User-Agent': 'dsh-manager' } });
    if (!res.ok) return null; // 403 限流或其它错误
    const j = await res.json();
    return j.items || [];
  } catch {
    return null;
  }
}

// 当前搜索的分页游标（单一会话状态；源或关键词变化时重置）。
let marketState = null; // { source, query, seen:Set, cursors:Map, rateLimited }

async function searchMarketPage(source, query, reset, coreDshPackages) {
  const q = String(query || '').trim();
  if (reset || !marketState || marketState.source !== source || marketState.query !== q) {
    marketState = { source, query: q, seen: new Set(), cursors: new Map(), rateLimited: false };
  }
  const st = marketState;
  const queries = source === 'github' ? githubQueries(q) : npmQueries(q);
  const items = [];
  let rateLimited = false;

  for (const key of queries) {
    let cur = st.cursors.get(key);
    if (!cur) {
      cur = { next: source === 'github' ? 1 : 0, exhausted: false };
      st.cursors.set(key, cur);
    }
    if (cur.exhausted) continue;

    const raw = source === 'github'
      ? await fetchGithubPage(key, cur.next)
      : await fetchNpmPage(key, cur.next);

    if (raw === null) {
      cur.exhausted = true;
      rateLimited = true; // 网络错误或限流：此 query 本次到此为止
      continue;
    }
    if (!raw.length) {
      cur.exhausted = true;
      continue;
    }
    cur.next += source === 'github' ? 1 : raw.length;
    if ((source === 'github' && cur.next > GH_MAX_PAGE) || (source === 'npm' && cur.next >= NPM_MAX_FROM)) {
      cur.exhausted = true;
    }

    for (const p of raw) {
      const id = source === 'github' ? p.full_name : p.name;
      if (!id || st.seen.has(id)) continue;
      st.seen.add(id);
      const item = source === 'github' ? githubItem(p) : npmItem(p);
      // npm：排除 dsh CLI 自身核心包，且要求关键词命中 dsh/deepseek/harness。
      if (source === 'npm') {
        if (coreDshPackages.has(item.name)) continue;
        if (!item.keywords.some((k) => /dsh|deepseek|harness/i.test(k))) continue;
      }
      items.push(item);
      if (items.length >= MARKET_BATCH) break;
    }
    if (items.length >= MARKET_BATCH) break;
  }

  st.rateLimited = st.rateLimited || rateLimited;
  const hasMore = items.length >= MARKET_BATCH ||
    queries.some((k) => { const c = st.cursors.get(k); return c && !c.exhausted; });
  return { items, hasMore, rateLimited: st.rateLimited, total: st.seen.size };
}

async function getGithubRepoStats(owner, repo) {
  try {
    const g = await fetch(`https://api.github.com/repos/${owner}/${repo}`, { headers: { 'User-Agent': 'dsh-manager' } }).then((r) => (r.ok ? r.json() : null));
    if (!g || g.message) return null;
    let readme = '';
    try {
      const r = await fetch(`https://raw.githubusercontent.com/${owner}/${repo}/HEAD/README.md`, { headers: { 'User-Agent': 'dsh-manager' } });
      if (r.ok) readme = (await r.text()).slice(0, 60000);
    } catch {
      // 忽略 README 失败。
    }
    return {
      fullName: g.full_name,
      stars: g.stargazers_count || 0,
      forks: g.forks_count || 0,
      openIssues: g.open_issues_count || 0,
      created: (g.created_at || '').slice(0, 10),
      pushed: (g.pushed_at || '').slice(0, 10),
      license: (g.license && g.license.spdx_id) || null,
      description: g.description || '',
      topics: g.topics || [],
      readme,
    };
  } catch {
    return null;
  }
}

async function getNpmPluginInfo(name) {
  try {
    const pack = await fetch(`https://registry.npmjs.org/${encodeURIComponent(name)}`, { headers: { 'User-Agent': 'dsh-manager' } }).then((r) => (r.ok ? r.json() : null));
    if (!pack || pack.error) return null;
    const latest = pack['dist-tags'] && pack['dist-tags'].latest;
    const v = pack.versions && pack.versions[latest];
    let downloads = null;
    try {
      const d = await fetch(`https://api.npmjs.org/downloads/point/last-week/${encodeURIComponent(name)}`).then((r) => r.json());
      downloads = d && d.downloads != null ? d.downloads : null;
    } catch {
      // 忽略下载量失败。
    }
    let repo = null;
    const repoUrl = (v && v.repository && v.repository.url) || '';
    const rm = repoUrl.match(/github\.com\/([^/]+)\/([^/.#]+)/);
    if (rm) repo = await getGithubRepoStats(rm[1], rm[2]);
    return {
      name: pack.name,
      version: latest,
      description: pack.description || '',
      keywords: pack.keywords || [],
      license: (v && v.license && (typeof v.license === 'string' ? v.license : v.license.type)) || null,
      homepage: (v && v.homepage) || pack.homepage || '',
      repository: repoUrl || '',
      created: ((pack.time && pack.time.created) || '').slice(0, 10),
      modified: ((pack.time && pack.time[latest]) || '').slice(0, 10),
      downloads,
      readme: (pack.readme || '').slice(0, 60000),
      repo,
      maintainers: (pack.maintainers || []).map((m) => m.name).slice(0, 5),
    };
  } catch {
    return null;
  }
}

async function getGithubPluginInfo(owner, repo) {
  const stats = await getGithubRepoStats(owner, repo);
  let pkg = null;
  try {
    const r = await fetch(`https://raw.githubusercontent.com/${owner}/${repo}/HEAD/package.json`, { headers: { 'User-Agent': 'dsh-manager' } });
    if (r.ok) pkg = await r.json();
  } catch {
    // 忽略。
  }
  return {
    repo: `${owner}/${repo}`,
    name: (pkg && pkg.name) || repo,
    pkgName: (pkg && pkg.name) || null,
    version: (pkg && pkg.version) || 'git',
    description: (stats && stats.description) || (pkg && pkg.description) || '',
    stats,
  };
}

module.exports = {
  searchMarketPage,
  getGithubRepoStats,
  getNpmPluginInfo,
  getGithubPluginInfo,
};
