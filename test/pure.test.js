'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const {
  parseVersion,
  compareVersions,
  normalizeGithubUrl,
  parseCimDate,
  releaseBodyToText,
  extractAnalysisJson,
  npmItem,
  githubItem,
} = require('../lib/pure');

test('compareVersions: numeric core ordering', () => {
  assert.equal(compareVersions('0.2.0', '0.1.9'), 1);
  assert.equal(compareVersions('0.1.9', '0.2.0'), -1);
  assert.equal(compareVersions('1.0.0', '1.0.0'), 0);
  assert.equal(compareVersions('1.0.1', '1.0.0'), 1);
  assert.equal(compareVersions('0.1.0', '0.10.0'), -1);
});

test('compareVersions: prerelease vs release', () => {
  assert.equal(compareVersions('1.0.0', '1.0.0-rc.1'), 1);
  assert.equal(compareVersions('1.0.0-rc.1', '1.0.0'), -1);
});

test('compareVersions: numeric prerelease identifiers (rc.10 > rc.9)', () => {
  assert.equal(compareVersions('0.1.0-rc.8', '0.1.0-rc.7'), 1);
  assert.equal(compareVersions('0.1.0-rc.10', '0.1.0-rc.9'), 1);
  assert.equal(compareVersions('0.1.0-rc.9', '0.1.0-rc.10'), -1);
});

test('compareVersions: mixed / string prerelease', () => {
  assert.equal(compareVersions('0.1.0-rc.1', '0.1.0-rc.2'), -1);
  assert.equal(compareVersions('0.1.0-alpha', '0.1.0-beta'), -1);
  // 数字段 > 字符串段
  assert.equal(compareVersions('0.1.0-1', '0.1.0-alpha'), 1);
  // 缺段者更小
  assert.equal(compareVersions('0.1.0-rc.1.2', '0.1.0-rc.1'), 1);
});

test('compareVersions: v prefix and whitespace tolerance', () => {
  assert.equal(compareVersions('v1.0.0', '1.0.0'), 0);
  assert.equal(compareVersions(' 1.0.0 ', '1.0.0'), 0);
  // 空字符串被解析为 0.0.0（调用方在比较前已用 current && latest 过滤空值）
  assert.equal(compareVersions('', '0.0.0'), 0);
});

test('parseVersion: shapes', () => {
  assert.deepEqual(parseVersion('1.2.3'), { nums: [1, 2, 3], pre: null });
  assert.deepEqual(parseVersion('1.2'), { nums: [1, 2, 0], pre: null });
  assert.deepEqual(parseVersion('1.0.0-rc.10'), { nums: [1, 0, 0], pre: ['rc', 10] });
});

test('normalizeGithubUrl: repo/homepage variants', () => {
  assert.equal(normalizeGithubUrl('git+https://github.com/owner/repo.git'), 'https://github.com/owner/repo');
  assert.equal(normalizeGithubUrl('https://github.com/owner/repo#readme'), 'https://github.com/owner/repo');
  assert.equal(normalizeGithubUrl('git+ssh://git@github.com/owner/repo.git'), 'https://github.com/owner/repo');
  assert.equal(normalizeGithubUrl('https://gitlab.com/owner/repo'), '');
  assert.equal(normalizeGithubUrl(''), '');
});

test('parseCimDate: WMI datetime', () => {
  assert.equal(parseCimDate('20260820093405.123456+480'), '2026-08-20 09:34:05');
  assert.equal(parseCimDate('  20260820093405  '), '2026-08-20 09:34:05');
  assert.equal(parseCimDate('nonsense'), null);
  assert.equal(parseCimDate(''), null);
});

test('releaseBodyToText: strips html/markdown and keeps Chinese section', () => {
  const body = '### v0.1.0-rc.8\n\n- 修复了 bug\n- 新增功能\n\n<h3 id="en">English section</h3>\n- English item';
  const out = releaseBodyToText(body);
  assert.ok(out.includes('修复了 bug'));
  assert.ok(out.includes('新增功能'));
  assert.ok(!out.includes('English item'));
  assert.ok(!out.includes('###')); // heading markers removed
});

test('releaseBodyToText: decodes entities and links', () => {
  const out = releaseBodyToText('见 [文档](https://example.com) &amp; **重点** `code`');
  assert.ok(out.includes('文档'));
  assert.ok(!out.includes('[文档]('));
  assert.ok(out.includes('&'));
  assert.ok(out.includes('重点'));
  assert.ok(out.includes('code'));
  assert.ok(!out.includes('<'));
});

test('extractAnalysisJson: picks last valid score object', () => {
  const text = 'some text\n{"score":3,"verdict":"一般"}\nmore\n{"score":8,"verdict":"真实有用"}';
  assert.deepEqual(extractAnalysisJson(text), { score: 8, verdict: '真实有用' });
});

test('extractAnalysisJson: ignores non-score objects and invalid json', () => {
  assert.equal(extractAnalysisJson('no json here'), null);
  assert.equal(extractAnalysisJson('{"foo":1}'), null);
  // 只要带 score/verdict 字段就接受（哪怕 score 是字符串）
  assert.deepEqual(extractAnalysisJson('{"score":"x","verdict":"真实有用"}'), { score: 'x', verdict: '真实有用' });
});

test('npmItem / githubItem: field mapping', () => {
  const n = npmItem({
    name: 'dsh-plugin', version: '1.0.0', description: 'd', keywords: ['dsh'],
    publisher: { username: 'u' }, date: '2026-01-02T00:00:00Z',
    links: { repository: 'git+https://github.com/o/r.git', homepage: '' },
  });
  assert.equal(n.scope, 'community');
  assert.equal(n.date, '2026-01-02');
  assert.equal(n.repoUrl, 'https://github.com/o/r');

  const g = githubItem({
    full_name: 'o/r', owner: { login: 'o' }, name: 'r', description: 'd',
    stargazers_count: 5, forks_count: 2, language: 'JS', updated_at: '2026-01-02T00:00:00Z',
    topics: ['dsh'], html_url: 'https://github.com/o/r',
  });
  assert.equal(g.stars, 5);
  assert.equal(g.url, 'https://github.com/o/r');
  assert.equal(g.updated, '2026-01-02');
});
