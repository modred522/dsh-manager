'use strict';
// 把版本号写进三处 manifest：package.json、src-tauri/Cargo.toml、
// src-tauri/tauri.conf.json。发版工作流从 tag 取值调用它。
//
// **为什么必须三处都写**：编译进 exe 的版本来自 Cargo.toml（`CARGO_PKG_VERSION`），
// 管理器的"检查管理器新版本"拿它跟发行页比较。Electron 时代只写 package.json 就够了
// （electron-builder 从那里取版本），照搬过来的话装出去的客户端会永远认为自己是仓库里
// 的占位版本 1.0.0，于是每次启动都报"有新版"。
//
// 三个文件都是**定点替换那一行**，不走 JSON.parse + stringify —— 后者会把
// tauri.conf.json 里紧凑写法的数组全展开，平白搅动几十行。
//
// 用法：node tools/set-version.js v1.0.7   （前面的 v 可有可无）

const fs = require('fs');

const raw = process.argv[2];
if (!raw) {
  console.error('用法: node tools/set-version.js <版本号>   例: node tools/set-version.js v1.0.7');
  process.exit(2);
}
const version = String(raw).trim().replace(/^v/, '');
// 宽松的 semver：主.次.补丁 + 可选的预发布后缀。
if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error(`版本号格式不对: ${raw}`);
  process.exit(2);
}

const PKG = 'package.json';
const CARGO = 'src-tauri/Cargo.toml';
const CONF = 'src-tauri/tauri.conf.json';

/** 替换文件里唯一一处匹配的行，保留原有行尾。 */
function replaceLine(file, matches, rewrite) {
  const src = fs.readFileSync(file, 'utf8');
  const eol = src.includes('\r\n') ? '\r\n' : '\n';
  const lines = src.split(/\r?\n/);
  const hits = lines.map((l, i) => (matches(l) ? i : -1)).filter((i) => i >= 0);
  if (hits.length !== 1) {
    console.error(`${file}: 期望恰好 1 处版本行，实际 ${hits.length} 处 —— 格式变了，这个脚本得跟着改`);
    process.exit(1);
  }
  lines[hits[0]] = rewrite(lines[hits[0]]);
  fs.writeFileSync(file, lines.join(eol));
}

// JSON 的顶层 version：靠"缩进两格"锁定层级，避免命中嵌套对象里的同名键。
const jsonVersionLine = (l) => /^ {2}"version"\s*:\s*"[^"]*"\s*,?\s*$/.test(l);
const setJsonVersion = (l) => l.replace(/"version"(\s*:\s*)"[^"]*"/, `"version"$1"${version}"`);
replaceLine(PKG, jsonVersionLine, setJsonVersion);
replaceLine(CONF, jsonVersionLine, setJsonVersion);

// Cargo.toml：只能动 [package] 段里那一行。依赖项也写 `version = "..."`，
// 不锁定段落的话会把整棵依赖树的版本号改成应用版本号。
{
  const src = fs.readFileSync(CARGO, 'utf8');
  const eol = src.includes('\r\n') ? '\r\n' : '\n';
  const lines = src.split(/\r?\n/);
  let section = '';
  let target = -1;
  for (let i = 0; i < lines.length; i++) {
    const s = lines[i].trim();
    if (s.startsWith('[')) section = s;
    else if (section === '[package]' && /^version\s*=/.test(s)) {
      target = i;
      break;
    }
  }
  if (target < 0) {
    console.error(`${CARGO}: [package] 段里找不到 version 行 —— 格式变了，这个脚本得跟着改`);
    process.exit(1);
  }
  lines[target] = lines[target].replace(/version\s*=\s*"[^"]*"/, `version = "${version}"`);
  fs.writeFileSync(CARGO, lines.join(eol));
}

// --- 读回来验一遍 ---
// 不自我校验的话，哪天 manifest 格式一变、匹配悄悄失效了，发版会"成功"
// 但装出去的客户端版本号是错的 —— 那种错很难发现。这里刻意用 JSON.parse /
// 段落解析来读，和上面的写入路径不同源。
const check = [
  [PKG, JSON.parse(fs.readFileSync(PKG, 'utf8')).version],
  [CONF, JSON.parse(fs.readFileSync(CONF, 'utf8')).version],
  [CARGO, (fs.readFileSync(CARGO, 'utf8').match(/\[package\][\s\S]*?\nversion\s*=\s*"([^"]+)"/) || [])[1]],
];
let bad = false;
for (const [file, got] of check) {
  const ok = got === version;
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${file}: ${got}`);
  if (!ok) bad = true;
}
if (bad) {
  console.error(`回读校验失败：有 manifest 没写成 ${version}`);
  process.exit(1);
}
console.log(`三处 manifest 均已写为 ${version}`);
