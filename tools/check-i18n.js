'use strict';
const fs = require('fs');

// 1. collect keys from i18n.js
const i18nSrc = fs.readFileSync('renderer/i18n.js', 'utf8');
const zhMatch = i18nSrc.match(/zh: \{([\s\S]*?)\n  \},/);
const enMatch = i18nSrc.match(/en: \{([\s\S]*?)\n  \},/);
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
console.log('zh-only:', zhOnly.length ? zhOnly.join(',') : '(none)');
console.log('en-only:', enOnly.length ? enOnly.join(',') : '(none)');

// 2. collect t('...') / data-i18n keys used in renderer/market JS + HTML
const used = new Set();
const files = ['renderer/renderer.js', 'renderer/market.js', 'renderer/index.html', 'renderer/market.html'];
for (const f of files) {
  const src = fs.readFileSync(f, 'utf8');
  for (const m of src.matchAll(/\bt\('([^']+)'/g)) used.add(m[1]);
  for (const m of src.matchAll(/data-i18n(?:-ph|-title)?="([^"]+)"/g)) used.add(m[1]);
}
console.log('used keys:', used.size);
const missing = [...used].filter((k) => !zhKeys.has(k));
console.log('USED but MISSING from dict:', missing.length ? missing.join(', ') : '(none)');
const unused = [...zhKeys].filter((k) => !used.has(k));
console.log('in dict but unused:', unused.length ? unused.join(', ') : '(none)');

// 3. id cross-check
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
for (const [jsPath, htmlPath] of [['renderer/renderer.js', 'renderer/index.html'], ['renderer/market.js', 'renderer/market.html']]) {
  const need = idsOf(fs.readFileSync(jsPath, 'utf8'));
  const have = htmlIds(fs.readFileSync(htmlPath, 'utf8'));
  const missingIds = [...need].filter((i) => !have.has(i));
  console.log(`${jsPath}: ids ${need.size}/${have.size}, missing: ${missingIds.length ? missingIds.join(',') : '(none)'}`);
}
