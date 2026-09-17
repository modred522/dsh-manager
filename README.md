# DSH 管理器

[![check](https://github.com/modred522/dsh-manager/actions/workflows/check.yml/badge.svg)](https://github.com/modred522/dsh-manager/actions/workflows/check.yml)
[![release](https://img.shields.io/github/v/release/modred522/dsh-manager)](https://github.com/modred522/dsh-manager/releases)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![tauri](https://img.shields.io/badge/Tauri-2-24C8D8.svg)](https://tauri.app/)

DeepSeek Harness（dsh）的桌面管理器：托盘常驻、一键启停、更新管理、插件市场与智能分析、Token 用量统计。

后端是 **Rust + Tauri 2**（原 Electron 版已退场，迁移过程见 `docs/TAURI-MIGRATION.md`）：安装器 2.3 MB、常驻内存约 40 MB。

[English](README.en.md) | 中文

## 截图

<table>
  <tr>
    <td align="center"><b>总览</b>（版本 / 进程 / 更新 / 设置 / 日志）<br><img src="assets/screenshots/overview.png" width="430" alt="总览"></td>
    <td align="center"><b>用量</b>（Token 统计 / 项目排行 / 14 天趋势）<br><img src="assets/screenshots/usage.png" width="430" alt="用量"></td>
  </tr>
  <tr>
    <td align="center"><b>插件</b>（已安装管理 / 快速安装）<br><img src="assets/screenshots/plugins.png" width="430" alt="插件"></td>
    <td align="center"><b>插件市场</b>（独立窗口，npm / GitHub 双源）<br><img src="assets/screenshots/market.png" width="430" alt="插件市场"></td>
  </tr>
  <tr>
    <td align="center" colspan="2"><b>插件详情 + 一键分析</b>（README / 评分卡 / 控制台，可拖拽分栏）<br><img src="assets/screenshots/analysis.png" width="720" alt="插件详情"></td>
  </tr>
</table>

## 功能

| 页签 | 功能 |
|---|---|
| 总览 | 打开/重启/停止 DSH（检测所有 dsh 进程）、进程面板（PID/内存/CPU/单个停止）、检查更新/立即更新/回滚（带官方 changelog）、设置（自启/守护/静默/主题/地址/间隔）、日志（持久化 7 天 + 导出）、工具（配置目录/安装目录/关于） |
| 用量 | Token 统计：总量、按项目排行、近 14 天趋势、费用估算（单价可调） |
| 插件 | 已安装插件管理 + 快速安装（npm）；**插件市场**（npm / GitHub 双源搜索）为**独立窗口**，点按钮打开；卡片直接展示 GitHub 页面链接（点击用系统浏览器打开）；插件详情为整页视图（README / 评分卡 / 控制台**可拖拽分栏**）、**一键分析**（dsh headless 评估"真实有用/徒有其表"，结构化评分卡 + 历史缓存）、安装/卸载（GitHub 源带供应链风险确认） |

## 快速开始

- **运行**：双击桌面「DSH 管理器」快捷方式；或直接运行安装目录下的 `DSH Manager.exe`。
- **退出**：托盘图标右键 → 退出（关闭窗口只是最小化到托盘）。
- **全局快捷键**：`Ctrl+Alt+D` 快速打开 DSH。

## 全新机器安装（源码）

```powershell
npm install -g @deepseek-ai/dsh      # 前置：安装 DeepSeek Harness CLI
# 另需 Rust stable（https://rustup.rs）与 WebView2 Runtime（Win11 自带）
git clone https://github.com/modred522/dsh-manager.git && cd dsh-manager
npm install                          # 只装 @tauri-apps/cli
npx tauri build                      # 出安装器；想直接跑开发模式用 npx tauri dev
```

## 安装（发行版 Release）

仍需要本机装有 **Node.js 与 dsh CLI**（管理器通过它们启动/更新 DSH）。

1. 到 [Releases](https://github.com/modred522/dsh-manager/releases) 下载最新的
   `DSH.Manager_<版本>_x64-setup.exe`（约 2.3 MB；GitHub 会把文件名里的空格换成点）
2. 双击安装（当前用户，无需管理员），自动创建桌面/开始菜单快捷方式
3. 之后管理器会在启动时**检查**有没有新版本，有就提示你到发行页下载 ——
   「总览 → 设置」里的「检查管理器新版本」开关控制这个行为

> **不做自动下载安装。** Tauri 的 updater 强制 Ed25519 签名，等于要长期保管一把
> 私钥，丢了所有已装客户端就再也无法自动更新；而 Windows 上它下载的本来也是
> 安装器、照样弹安装界面。理由详见 `docs/TAURI-MIGRATION.md` 第十六节。

发版（维护者）：**推 tag 自动发版**——GitHub Actions 在服务器上自动打包并创建 Release：

```powershell
git tag v1.0.2
git push origin v1.0.2
```

本地手动打包（备用）：

```powershell
npx tauri build   # 产物在 src-tauri\target\release\bundle\nsis\
```

发版工作流会用 `tools/set-version.js` 把 tag 里的版本号写进 `package.json`、
`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json` 三处 —— 少写 Cargo.toml 的话，
装出去的客户端会永远认为自己是仓库里的占位版本。

## 目录结构

```
dsh-manager
├── src-tauri/              # Rust 后端
│   ├── src/                # lib.rs（命令与外壳）+ 13 个模块，详见 ARCHITECTURE.md
│   ├── capabilities/       # Tauri 2 的权限（必须自建，缺了前端收不到任何事件）
│   ├── tauri.conf.json     # 应用元数据与 NSIS 打包配置
│   └── Cargo.toml
├── renderer/               # 前端（原生 JS，无框架、无构建步骤）
│   ├── index.html + renderer.js + styles.css + i18n.js
│   ├── market.html + market.js      # 插件市场独立窗口
│   └── tauri-bridge.js              # 把 window.dsh.* 架在 invoke/listen 上
├── .github/workflows/      # check（cargo 三件套 + 渲染层自检）/ release（推 v* tag 自动发版）
├── tools/
│   ├── check-i18n.js       # 渲染层自检：i18n 键集 / DOM id / 全局遮蔽 / CSS 溢出
│   ├── set-version.js      # 发版时把版本号写进三处 manifest
│   ├── render-icon.ps1     # 鲸鱼图标生成（STA 运行）
│   ├── social-preview.ps1  # GitHub 社交预览卡生成（1280x640）
│   └── publish.ps1         # 一键发布到 GitHub
├── assets/                 # whale.png / app.ico / social-preview.png / screenshots/
└── docs/                   # MEMORY.md（交接）/ ARCHITECTURE.md / GOTCHAS.md
                            # TAURI-MIGRATION.md（Electron→Tauri 全过程）
```

## 常用开发命令

```powershell
npm install                     # 只装 @tauri-apps/cli
npx tauri dev                   # 开发模式（热加载前端）
npx tauri build                 # 出 NSIS 安装器
node tools\check-i18n.js        # 渲染层自检（i18n / DOM id / 全局遮蔽 / CSS 溢出）

cd src-tauri
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test

powershell -NoProfile -STA -File tools\render-icon.ps1  # 重新生成鲸鱼图标
powershell -NoProfile -File tools\publish.ps1  # fork 后一键发布到自己的 GitHub 仓库
```

> **别把 `tauri dev` 的产物拿去建桌面快捷方式。** debug 构建是控制台子系统（会弹黑框），
> 前端又指向 dev 服务器，脱离 `tauri dev` 打开只会显示"无法访问此页面"。
> 应用本身已经拒绝在 debug 构建下写快捷方式。

## 相关文档

- **`docs/MEMORY.md`**：项目交接记忆（新会话必读）
- **`docs/ARCHITECTURE.md`**：代码地图、IPC 契约、数据结构、扩展指南
- **`docs/GOTCHAS.md`**：踩坑记录与交付检查清单

## 依赖

- 运行：Windows 10/11 + WebView2 Runtime（Win11 自带）
- 开发：Rust stable（1.80+）、Node.js 24 / npm（只为装 `@tauri-apps/cli`）
- dsh（npm 全局，`@deepseek-ai/dsh`）、DSH_HOME 默认 `~\.dsh`

## 致谢

- 鲸鱼图标由 DeepSeek Harness（dsh，MIT License）的 favicon 路径渲染生成。

## 许可证

[MIT](LICENSE)
