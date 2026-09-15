'use strict';
// electron-builder afterPack 钩子：在打包完成、**发布之前**核对产物完整性。
// 抛异常会让 electron-builder 直接中止，所以坏产物不会被推上 Release。
//
// 光靠 CI 里的静态核对（tools/check-package.js 白名单模式）只能拦住"漏列 files"，
// 这里再验一遍真实产物目录，把任何打包期的意外（glob 没匹配上、文件被过滤器吃掉）也拦下。

const { execFileSync } = require('node:child_process');
const path = require('node:path');

module.exports = async function afterPack(context) {
  // asar: false，应用文件直接落在 resources/app 下。
  const appDir = path.join(context.appOutDir, 'resources', 'app');
  const script = path.join(__dirname, 'check-package.js');

  console.log(`  • 核对产物完整性  appDir=${appDir}`);
  try {
    const out = execFileSync(process.execPath, [script, '--packaged', appDir], { encoding: 'utf8' });
    process.stdout.write(out);
  } catch (e) {
    if (e.stdout) process.stdout.write(e.stdout);
    if (e.stderr) process.stderr.write(e.stderr);
    throw new Error('打包产物完整性核对失败，已中止发布（见上方缺失文件清单）');
  }
};
