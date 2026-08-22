'use strict';
// 纯函数模块：无 Electron / 文件系统 / 网络 / 全局状态依赖，可被 node:test 直接测试。

// 解析版本号为 { nums, pre }。pre 为逐段数组（数字段转 number，字符串段保留字符串），无预发布段时为 null。
function parseVersion(v) {
  const s = String(v || '').trim().replace(/^v/, '');
  const [core, ...preParts] = s.split('-');
  const nums = (core || '').split('.').map((n) => parseInt(n, 10) || 0);
  while (nums.length < 3) nums.push(0);
  const pre = preParts.length
    ? preParts.join('-').split('.').map((p) => (/^\d+$/.test(p) ? parseInt(p, 10) : p))
    : null;
  return { nums, pre };
}

// semver 规则比较两个版本（预发布段逐段：数字段 > 字符串段，缺段者更小）。
// 例：rc.10 > rc.9（纯字符串比较会错）；1.0.0 > 1.0.0-rc.1。
function compareVersions(a, b) {
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
  const x = parseVersion(a);
  const y = parseVersion(b);
  for (let i = 0; i < 3; i++) {
    if (x.nums[i] !== y.nums[i]) return x.nums[i] > y.nums[i] ? 1 : -1;
  }
  return cmpPre(x.pre, y.pre);
}

// 从仓库/主页 URL 中提取干净的 GitHub 页面地址。
function normalizeGithubUrl(url) {
  const s = String(url || '');
  const m = s.match(/github\.com\/([^/]+)\/([^/?#.]+)/);
  return m ? `https://github.com/${m[1]}/${m[2]}` : '';
}

// WMI CreationDate（CIM datetime，形如 20260820093405.123456+480）→ 'YYYY-MM-DD HH:MM:SS'。
function parseCimDate(s) {
  const m = String(s || '').trim().match(/^(\d{4})(\d{2})(\d{2})(\d{2})(\d{2})(\d{2})/);
  if (!m) return null;
  return `${m[1]}-${m[2]}-${m[3]} ${m[4]}:${m[5]}:${m[6]}`;
}

// GitHub Release 正文 → 纯文本 changelog（只保留中文段，清理 markdown/HTML 标记）。
function releaseBodyToText(body) {
  let text = String(body || '');
  const enIdx = text.indexOf('<h3 id="en">');
  if (enIdx !== -1) text = text.slice(0, enIdx);
  text = text.replace(/<h3[^>]*>([\s\S]*?)<\/h3>/gi, (_m, inner) => '\n■ ' + inner.replace(/<[^>]+>/g, '').replace(/\[[^\]]*\]/g, '').trim() + '\n');
  text = text.replace(/^###\s+/gm, '\n■ ');
  text = text.replace(/<[^>]+>/g, '');
  text = text.replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'");
  text = text.replace(/\[([^\]]*)\]\([^)]*\)/g, '$1');
  text = text.replace(/\*\*([^*]+)\*\*/g, '$1');
  text = text.replace(/^\s*[-*]\s+/gm, '  • ');
  text = text.replace(/`([^`]*)`/g, '$1');
  text = text.replace(/^\s*---+\s*$/gm, '');
  return text.split('\n').map((l) => l.trimEnd()).join('\n').trim();
}

// 从模型输出里提取最后一个可解析的 JSON 评分卡（含 score 或 verdict 字段）。
function extractAnalysisJson(text) {
  const blocks = [...String(text || '').matchAll(/\{[\s\S]*?\}/g)].map((m) => m[0]).reverse();
  for (const b of blocks) {
    try {
      const obj = JSON.parse(b);
      if (obj && (typeof obj.score !== 'undefined' || obj.verdict)) return obj;
    } catch {
      // 继续尝试。
    }
  }
  return null;
}

// npm 搜索条目 → 市场卡片字段。
function npmItem(p) {
  return {
    name: p.name,
    version: p.version,
    description: p.description || '',
    keywords: p.keywords || [],
    publisher: (p.publisher && p.publisher.username) || '',
    date: (p.date || '').slice(0, 10),
    scope: p.name.startsWith('@deepseek-ai/') ? 'official' : 'community',
    homepage: (p.links && p.links.homepage) || '',
    repoUrl: normalizeGithubUrl((p.links && p.links.repository) || '') ||
      normalizeGithubUrl((p.links && p.links.homepage) || ''),
  };
}

// GitHub 搜索条目 → 市场卡片字段。
function githubItem(it) {
  return {
    repo: it.full_name,
    owner: (it.owner && it.owner.login) || '',
    name: it.name,
    description: it.description || '',
    stars: it.stargazers_count || 0,
    forks: it.forks_count || 0,
    language: it.language || '',
    updated: (it.updated_at || '').slice(0, 10),
    topics: it.topics || [],
    url: it.html_url || '',
  };
}

module.exports = {
  parseVersion,
  compareVersions,
  normalizeGithubUrl,
  parseCimDate,
  releaseBodyToText,
  extractAnalysisJson,
  npmItem,
  githubItem,
};
