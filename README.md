# DSH 管理器

DeepSeek Harness（dsh）的 Electron 桌面管理器：托盘常驻、一键启停、更新管理、插件市场与智能分析、Token 用量统计。

## 快速开始

- **运行**：双击桌面「DSH 管理器」快捷方式；或在项目目录执行 `npm start`。
- **退出**：托盘图标右键 → 退出（关闭窗口只是最小化到托盘）。
- **全局快捷键**：`Ctrl+Alt+D` 快速打开 DSH。

## 功能

| 页签 | 功能 |
|---|---|
| 总览 | 打开/重启/停止 DSH（检测所有 dsh 进程）、进程面板（PID/内存/CPU/单个停止）、检查更新/立即更新/回滚（带官方 changelog）、设置（自启/守护/静默/主题/地址/间隔）、日志（持久化 7 天 + 导出）、工具（配置目录/安装目录/关于） |
| 用量 | Token 统计：总量、按项目排行、近 14 天趋势、费用估算（单价可调） |
| 插件 | 已安装插件管理 + 快速安装（npm）；**插件市场**（npm / GitHub 双源搜索）为**独立窗口**，点按钮打开；卡片直接展示 GitHub 页面链接（点击用系统浏览器打开）；插件详情为整页视图（README / 评分卡 / 控制台**可拖拽分栏**）、**一键分析**（dsh headless 评估"真实有用/徒有其表"，结构化评分卡 + 历史缓存）、安装/卸载（GitHub 源带供应链风险确认） |

## 目录结构

```
D:\DSHManager
├── main.js            # 主进程全部逻辑
├── preload.js         # IPC 桥（window.dsh）
├── find-dsh.ps1       # dsh 进程 WMI 详情（纯 ASCII）
├── tools/render-icon.ps1  # 鲸鱼图标生成（STA 运行）
├── assets/            # whale.png / app.ico
├── renderer/          # index.html + styles.css + renderer.js（主窗口）
│                      # + market.html + market.js（插件市场独立窗口）
└── docs/              # MEMORY.md（交接）/ ARCHITECTURE.md / GOTCHAS.md
```

## 常用开发命令

```powershell
npm start                                  # 启动应用
node --check main.js                       # 语法检查（preload.js、renderer/renderer.js 同理）
Get-Process electron | Stop-Process -Force # 改完代码杀旧实例
powershell -NoProfile -STA -File tools\render-icon.ps1  # 重新生成鲸鱼图标
```

## 相关文档

- **`docs/MEMORY.md`**：项目交接记忆（新会话必读）
- **`docs/ARCHITECTURE.md`**：代码地图、IPC 契约、数据结构、扩展指南
- **`docs/GOTCHAS.md`**：踩坑记录与交付检查清单

## 依赖

- Node.js 24 / npm 11 / Electron 43.4.0（`npm install` 安装）
- dsh（npm 全局，`@deepseek-ai/dsh`）、DSH_HOME 默认 `~\.dsh`

## 全新机器安装

```powershell
npm install -g @deepseek-ai/dsh      # 前置：安装 DeepSeek Harness CLI
git clone <仓库地址> && cd dsh-manager
npm install
npm start
```

## 发布到 GitHub（公共仓库）

项目自带发布脚本（初始化 git、打初始提交、建公共仓库并推送）：

```powershell
powershell -NoProfile -File tools\publish.ps1          # 走 GitHub CLI（推荐：winget install GitHub.cli + gh auth login）
powershell -NoProfile -File tools\publish.ps1 -Remote https://github.com/<你>/dsh-manager.git  # 手动路线：先建空仓库
```

许可证：MIT（见 `LICENSE`）。

## 致谢

- 鲸鱼图标由 DeepSeek Harness（dsh，MIT License）的 favicon 路径渲染生成。
