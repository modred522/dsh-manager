# DSH 管理器

[![check](https://github.com/modred522/dsh-manager/actions/workflows/check.yml/badge.svg)](https://github.com/modred522/dsh-manager/actions/workflows/check.yml)
[![release](https://img.shields.io/github/v/release/modred522/dsh-manager)](https://github.com/modred522/dsh-manager/releases)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![electron](https://img.shields.io/badge/Electron-43.4.0-47848F.svg)](https://www.electronjs.org/)

DeepSeek Harness（dsh）的 Electron 桌面管理器：托盘常驻、一键启停、更新管理、插件市场与智能分析、Token 用量统计。

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

- **运行**：双击桌面「DSH 管理器」快捷方式；或在项目目录执行 `npm start`。
- **退出**：托盘图标右键 → 退出（关闭窗口只是最小化到托盘）。
- **全局快捷键**：`Ctrl+Alt+D` 快速打开 DSH。

## 全新机器安装（源码）

```powershell
npm install -g @deepseek-ai/dsh      # 前置：安装 DeepSeek Harness CLI
git clone https://github.com/modred522/dsh-manager.git && cd dsh-manager
npm install
npm start
```

## 安装（发行版 Release）

仍需要本机装有 **Node.js 与 dsh CLI**（管理器通过它们启动/更新 DSH）。

**方式一：安装器（推荐，带自动更新）**

1. 到 [Releases](https://github.com/modred522/dsh-manager/releases) 下载最新的 `DSH-Manager-Setup-<版本>.exe`
2. 双击安装，可选安装目录、自动创建桌面/开始菜单快捷方式
3. 之后管理器会自动检测并下载新版本（「总览 → 设置」里的「自动更新管理器」开关），下载完成提示重启即升级

**方式二：便携版 zip（解压即用）**

1. 下载 `DSH-Manager-<版本>-win.zip`
2. 解压到任意目录，双击 `DSH Manager.exe`（便携版不含自动更新，手动下载新版覆盖即可）

发版（维护者）：**推 tag 自动发版**——GitHub Actions 在服务器上自动打包并创建 Release：

```powershell
git tag v1.0.2
git push origin v1.0.2
```

本地手动打包（备用）：输出到项目内 `DSHManagerRelease\release`（已 gitignore），不污染源码树：

```powershell
powershell -NoProfile -File tools\build-release.ps1 -Version 1.0.2
```

## 目录结构

```
dsh-manager
├── main.js                 # 主进程全部逻辑
├── preload.js              # IPC 桥（window.dsh）
├── find-dsh.ps1            # dsh 进程 WMI 详情（纯 ASCII）
├── .github/workflows/      # CI（push/PR 自动语法检查）
├── tools/
│   ├── render-icon.ps1     # 鲸鱼图标生成（STA 运行）
│   ├── social-preview.ps1  # GitHub 社交预览卡生成（1280x640）
│   ├── build-release.ps1   # 独立目录打包发行 zip
│   └── publish.ps1         # 一键发布到 GitHub
├── assets/                 # whale.png / app.ico / social-preview.png / screenshots/
├── renderer/               # 主窗口：index.html + styles.css + renderer.js
│                           # 插件市场独立窗口：market.html + market.js
└── docs/                   # MEMORY.md（交接）/ ARCHITECTURE.md / GOTCHAS.md
```

## 常用开发命令

```powershell
npm start                                  # 启动应用
npm test                                   # 纯函数单元测试（node:test）
node --check main.js                       # 语法检查（preload.js、lib/*.js、renderer/*.js 同理）
Get-Process electron | Stop-Process -Force # 改完代码杀旧实例
powershell -NoProfile -STA -File tools\render-icon.ps1  # 重新生成鲸鱼图标
powershell -NoProfile -File tools\publish.ps1  # fork 后一键发布到自己的 GitHub 仓库
```

## 相关文档

- **`docs/MEMORY.md`**：项目交接记忆（新会话必读）
- **`docs/ARCHITECTURE.md`**：代码地图、IPC 契约、数据结构、扩展指南
- **`docs/GOTCHAS.md`**：踩坑记录与交付检查清单

## 依赖

- Node.js 24 / npm 11 / Electron 43.4.0（`npm install` 安装）
- dsh（npm 全局，`@deepseek-ai/dsh`）、DSH_HOME 默认 `~\.dsh`

## 致谢

- 鲸鱼图标由 DeepSeek Harness（dsh，MIT License）的 favicon 路径渲染生成。

## 许可证

[MIT](LICENSE)
