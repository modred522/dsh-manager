'use strict';
// 打包完整性核对：从入口文件出发递归跟踪相对 require()，确认每个被依赖的本地
// 模块都落在 electron-builder.yml 的 files 白名单内。
//
// 存在意义：electron-builder 的 files 是白名单 + 本项目 asar:false，漏列一个
// 目录不会有任何构建报错，但装出来的应用一启动就 "Cannot find module"。
// v1.0.5 就是这么发出去的（lib/ 没列进 files），CI 的 node --check 拦不住。
//
// 用法:
//   node tools/check-package.js                       # 核对源码树 vs files 白名单
//   node tools/check-package.js --packaged <app 目录>  # 核对已打包产物里文件是否齐全

const fs = require('node:fs');
const path = require('node:path');

const ROOT = path.join(__dirname, '..');
const ENTRIES = ['main.js', 'preload.js'];

// electron-builder.yml 的 files 段（只需读一个简单的字符串数组，不引 YAML 依赖）。
function readFilesWhitelist() {
  const text = fs.readFileSync(path.join(ROOT, 'electron-builder.yml'), 'utf8');
  const lines = text.split(/\r?\n/);
  const start = lines.findIndex((l) => /^files:\s*$/.test(l));
  if (start === -1) throw new Error('electron-builder.yml 里找不到 files: 段');
  const out = [];
  for (const line of lines.slice(start + 1)) {
    const m = line.match(/^\s+-\s+(.+?)\s*$/);
    if (!m) break; // 缩进列表结束
    out.push(m[1].replace(/^['"]|['"]$/g, ''));
  }
  return out;
}

// 把 electron-builder 的 glob 转成正则（只需支持本项目用到的 ** / * / 具名路径）。
function globToRegExp(pattern) {
  let re = '';
  for (let i = 0; i < pattern.length; i++) {
    const c = pattern[i];
    if (c === '*') {
      if (pattern[i + 1] === '*') {
        re += '.*';
        i++;
        if (pattern[i + 1] === '/') i++; // `lib/**/x` 里的斜杠已被 .* 吸收
      } else {
        re += '[^/]*';
      }
    } else if (c === '?') {
      re += '[^/]';
    } else {
      re += c.replace(/[.+^${}()|[\]\\]/g, '\\$&');
    }
  }
  return new RegExp('^' + re + '$');
}

function isWhitelisted(relPath, patterns) {
  const p = relPath.split(path.sep).join('/');
  return patterns.some((pat) => globToRegExp(pat).test(p));
}

// 解析一个相对 require 说明符到实际文件（补 .js / index.js，与 Node 解析一致）。
function resolveLocal(fromFile, spec) {
  const base = path.resolve(path.dirname(fromFile), spec);
  for (const cand of [base, base + '.js', base + '.json', path.join(base, 'index.js')]) {
    if (fs.existsSync(cand) && fs.statSync(cand).isFile()) return cand;
  }
  return null;
}

// 从入口出发递归收集所有被 require 的本地文件（相对路径形式）。
function collectLocalDeps() {
  const seen = new Set();
  const missing = [];
  const queue = ENTRIES.map((e) => path.join(ROOT, e));

  while (queue.length) {
    const file = queue.shift();
    const rel = path.relative(ROOT, file);
    if (seen.has(rel)) continue;
    seen.add(rel);

    let src;
    try {
      src = fs.readFileSync(file, 'utf8');
    } catch {
      missing.push({ from: '(入口)', spec: rel, reason: '文件不存在' });
      continue;
    }
    for (const m of src.matchAll(/require\(\s*['"](\.[^'"]+)['"]\s*\)/g)) {
      const target = resolveLocal(file, m[1]);
      if (!target) {
        missing.push({ from: rel, spec: m[1], reason: '解析不到文件' });
        continue;
      }
      queue.push(target);
    }
  }
  return { files: [...seen], missing };
}

function main() {
  const packagedIdx = process.argv.indexOf('--packaged');
  const packagedDir = packagedIdx !== -1 ? process.argv[packagedIdx + 1] : null;

  const { files, missing } = collectLocalDeps();
  const problems = [];

  for (const m of missing) {
    problems.push(`require 解析失败: ${m.from} → '${m.spec}'（${m.reason}）`);
  }

  if (packagedDir) {
    // 产物模式：被依赖的文件必须真实存在于打包目录里。
    for (const rel of files) {
      if (!fs.existsSync(path.join(packagedDir, rel))) {
        problems.push(`打包产物缺少被 require 的文件: ${rel}`);
      }
    }
    console.log(`模式: 产物核对 (${packagedDir})`);
  } else {
    // 源码模式：被依赖的文件必须命中 files 白名单。
    const patterns = readFilesWhitelist();
    for (const rel of files) {
      if (!isWhitelisted(rel, patterns)) {
        problems.push(`被 require 但没列进 electron-builder.yml 的 files: ${rel}`);
      }
    }
    console.log('模式: files 白名单核对');
    console.log('白名单: ' + patterns.join(', '));
  }

  console.log(`入口依赖的本地模块 ${files.length} 个: ${files.join(', ')}`);

  if (problems.length) {
    console.error('\n打包完整性核对失败:');
    for (const p of problems) console.error('  ✗ ' + p);
    process.exit(1);
  }
  console.log('\n✓ 打包完整性核对通过');
}

main();
