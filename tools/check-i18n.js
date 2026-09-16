'use strict';
// 渲染层静态自检：i18n 字典对齐、DOM id 对齐、共享全局名不被遮蔽。
//
// 退出码非 0 才算发现问题，CI 靠这个卡住合并。纯提示性的结论（字典里有
// 但没人用的 key）只打印，不算失败。
const fs = require('fs');

const failures = [];
const fail = (msg) => {
  failures.push(msg);
  console.log(`FAIL  ${msg}`);
};

const read = (p) => fs.readFileSync(p, 'utf8');
const I18N_PATH = 'renderer/i18n.js';
// 三份脚本按 <script> 顺序注入同一个全局作用域：i18n.js 先，然后 renderer.js
// 或 market.js（两者分属不同窗口，从不同时加载，所以只有 i18n.js 的全局是共享的）。
const CONSUMERS = ['renderer/renderer.js', 'renderer/market.js'];

// ---------------------------------------------------------------------------
// 1. 字典对齐：zh / en 键集必须一致，用到的 key 必须存在
// ---------------------------------------------------------------------------
const i18nSrc = read(I18N_PATH);
const zhMatch = i18nSrc.match(/zh: \{([\s\S]*?)\n  \},/);
const enMatch = i18nSrc.match(/en: \{([\s\S]*?)\n  \},/);
if (!zhMatch || !enMatch) {
  fail(`${I18N_PATH}: 解析不出 zh / en 字典块（结构变了就得改这个脚本）`);
  process.exit(1);
}
const keysOf = (block) => {
  const out = new Set();
  const re = /(?:^|[,{])\s*(\w+):/gm;
  let m;
  while ((m = re.exec(block))) out.add(m[1]);
  return out;
};
const zhKeys = keysOf(zhMatch[1]);
const enKeys = keysOf(enMatch[1]);
console.log('zh keys:', zhKeys.size, '| en keys:', enKeys.size);

const zhOnly = [...zhKeys].filter((k) => !enKeys.has(k));
const enOnly = [...enKeys].filter((k) => !zhKeys.has(k));
if (zhOnly.length) fail(`只有中文、缺英文: ${zhOnly.join(', ')}`);
if (enOnly.length) fail(`只有英文、缺中文: ${enOnly.join(', ')}`);
if (!zhOnly.length && !enOnly.length) console.log('zh/en 键集对齐: ok');

const used = new Set();
for (const f of [...CONSUMERS, 'renderer/index.html', 'renderer/market.html']) {
  const src = read(f);
  for (const m of src.matchAll(/\bt\('([^']+)'/g)) used.add(m[1]);
  for (const m of src.matchAll(/data-i18n(?:-ph|-title)?="([^"]+)"/g)) used.add(m[1]);
}
console.log('used keys:', used.size);
// 用到但字典里没有 —— t() 会原样返回 key，界面上直接露出 "btnFoo" 这种字符串。
const missing = [...used].filter((k) => !zhKeys.has(k));
if (missing.length) fail(`用到但字典里没有: ${missing.join(', ')}`);
const unused = [...zhKeys].filter((k) => !used.has(k));
console.log('字典里有但没人用（仅提示）:', unused.length ? unused.join(', ') : '(none)');

// ---------------------------------------------------------------------------
// 2. DOM id 对齐：$('x') 取不到元素，后面访问 .textContent 就是一次崩溃
// ---------------------------------------------------------------------------
const idsOf = (js) => {
  const out = new Set();
  for (const m of js.matchAll(/\$\('([^']+)'\)/g)) out.add(m[1]);
  return out;
};
const htmlIds = (html) => {
  const out = new Set();
  for (const m of html.matchAll(/id="([^"]+)"/g)) out.add(m[1]);
  return out;
};
for (const [jsPath, htmlPath] of [
  ['renderer/renderer.js', 'renderer/index.html'],
  ['renderer/market.js', 'renderer/market.html'],
]) {
  const need = idsOf(read(jsPath));
  const have = htmlIds(read(htmlPath));
  const missingIds = [...need].filter((i) => !have.has(i));
  console.log(`${jsPath}: ids ${need.size}/${have.size}`);
  if (missingIds.length) fail(`${htmlPath} 缺少 ${jsPath} 要取的 id: ${missingIds.join(', ')}`);
}

// ---------------------------------------------------------------------------
// 3. 共享全局不被遮蔽
// ---------------------------------------------------------------------------
// 真实事故：renderUsage 里写了 `const t = usage.totals || {}`，把 i18n 的 t()
// 遮成了一个数据对象，同一函数后面的 t('projEmpty') 抛 "t is not a function"。
// 因为它在 async 调用链里，界面只是静默少了项目排行和趋势图 —— 从初版一直带到现在。
//
// 保留名直接从 i18n.js 的顶层声明里抽，将来那边加了全局这里自动跟上。
const reserved = new Set();
for (const m of i18nSrc.matchAll(/^(?:const|let|var)\s+(\w+)/gm)) reserved.add(m[1]);
for (const m of i18nSrc.matchAll(/^function\s+(\w+)/gm)) reserved.add(m[1]);
console.log('i18n.js 导出的共享全局:', [...reserved].join(', '));

for (const f of CONSUMERS) {
  const lines = read(f).split('\n');
  for (const name of reserved) {
    const n = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const patterns = [
      // 声明：const t = ... / let t / var t
      [new RegExp(`\\b(?:const|let|var)\\s+${n}\\b`), '声明同名变量'],
      // 同名函数
      [new RegExp(`\\bfunction\\s+${n}\\s*\\(`), '声明同名函数'],
      // 箭头函数单参数：(t) => ... / t => ...
      [new RegExp(`(?:\\(\\s*${n}\\s*\\)|(?:^|[\\s(,=])${n})\\s*=>`), '用作箭头函数参数'],
      // for (const t of ...) 已被上面的声明规则覆盖；catch (t) 单列
      [new RegExp(`\\bcatch\\s*\\(\\s*${n}\\s*\\)`), '用作 catch 参数'],
    ];
    lines.forEach((line, i) => {
      if (/^\s*(\/\/|\*)/.test(line)) return; // 注释行不算
      for (const [re, what] of patterns) {
        if (re.test(line)) fail(`${f}:${i + 1} ${what} "${name}"，会遮蔽 i18n.js 的同名全局`);
      }
    });
  }
}
if (!failures.length) console.log('共享全局未被遮蔽: ok');

// ---------------------------------------------------------------------------
console.log('');
if (failures.length) {
  console.log(`${failures.length} 项检查未通过。`);
  process.exit(1);
}
console.log('渲染层自检全部通过。');
