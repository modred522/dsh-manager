# DSH 管理器 — 项目交接记忆（新会话必读）

> 新会话接入顺序：**先读本文件** → 按需读 `docs/ARCHITECTURE.md`（代码地图）和 `docs/GOTCHAS.md`（踩坑记录）。

## 一、这是什么

一个 **Electron 桌面管理器**，管理本机的 DeepSeek Harness（dsh）：

- 系统托盘常驻 + 三页签界面（总览 / 用量 / 插件）
- 一键启动/重启/停止 DSH（检测**所有** dsh 进程，不只是自己启动的）
- 检查更新 / 一键更新 / 回滚（带官方 changelog）
- **插件市场**（npm + GitHub 双源）+ **一键分析**（调用 dsh headless 评估插件"真实有用还是徒有其表"）
- Token 用量仪表盘（按项目/按天 + 费用估算）、日志持久化、崩溃守护、深色模式等

## 二、开发时间线（关键节点）

1. **.NET WPF 原型**（旧工作区，已废弃删除）：托盘、打开 DSH、检查/更新、自启、快捷方式。启动崩溃过一次（复选框事件在定时器初始化前触发 NRE）。
2. **Electron 重构**（用户要求不用原生 .NET）→ `dsh-manager-electron`。
3. **图标与 UI**：从 `dsh-web-frontend/dist/favicon.svg` 提取鲸鱼路径，WPF 渲染多尺寸 PNG + 手工组装 PNG 条目 ICO。
4. **检测 dsh 进程**：经历 WMI 命令行匹配失败 → 最终方案 **netstat 端口检测（Node 直连）+ WMI 详情（30 秒缓存）**。
5. **UI 动效全套 + 进程面板 + 更新日志 + 技术优化 + 快捷入口**（第一轮功能增强）。
6. **Token 用量仪表盘 + 插件管理**（数据源 `session_projcache.json` 与 web profile 的 package.json）。
7. **小优化**：更新后自动重启+回滚、日志持久化、崩溃守护、任务栏进度、托盘动态状态、深色模式+窗口记忆。
8. **插件市场 + 一键分析**（重功能，Phase 0 实测验证后实现）。
9. **搬迁**：项目迁到 **`D:\DSHManager`**；桌面快捷方式重定向；旧工作区管理器残留已清理。
10. **插件市场独立窗口**：市场从插件页签迁出，改为**独立 BrowserWindow**（`renderer/market.html` + `market.js`，插件页按钮打开、重复点击聚焦）；卡片直接展示 GitHub 页面链接（npm 源无仓库信息时回退 npm 页面），点击经 `open-external` 用系统浏览器打开；`log`/`state`/`analyze-log`/`analyze-done` 事件改为 `broadcast()` 广播到主窗口+市场窗口。
11. **插件详情整页视图 + 可拖拽分栏**：详情从 640px 弹窗改为市场窗口整页视图（返回按钮切换）；README（原始数据）左栏、评分卡 + 分析控制台右栏上下分栏，分割条拖拽调大小（pointer capture 实现），比例存 localStorage、双击复位。
12. **市场无限滚动分页**：搜索改为 `searchMarketPage()` 游标式分页（npm `from`/`size`、GitHub `page`+`sort=stars`；每批约 20 条），渲染层滑到底部自动加载更多、底部状态条提示（"已加载全部/点击重试"）；**排序**：npm 走 registry 默认相关度（质量/维护/流行度综合分），GitHub 固定星标降序。
13. **分析会话隔离**：headless 分析 spawn 加官方 `--patch` 层，把 `session-persistence-jsonl.root` 重定向到管理器私有目录 `%APPDATA%\DshManager\analysis-sessions\`（分析会话不再出现在 dsh web 聊天列表和用量统计）；分析结束按配置清理该目录（设置项"分析后清理分析会话"默认开）；启动时校验 dsh 版本的行 id 并清理崩溃遗留。**约定：绝不删除 `$DSH_HOME/sessions` 下的用户会话（用户自己清理）。**
14. **发布 GitHub 公共仓库**：`https://github.com/modred522/dsh-manager`（MIT）；CI = `.github/workflows/check.yml`（push 自动 node --check）；`tools/publish.ps1` 发布、`tools/social-preview.ps1` 生成 1280x640 社交预览卡（Settings→Social preview 手动上传）、`tools/build-release.ps1` 在项目内独立目录 `DSHManagerRelease`（**已 gitignore**）打 win-x64 便携 zip（git archive 快照 + asar:false，electron-builder）；打包版快捷方式/自启不带项目目录参数（`app.isPackaged` 判断）；Release 发布：v1.0.0。
15. **任务栏图标修复 + 中英双语**：`app.setAppUserModelId('com.modred522.dsh-manager')`（任务栏图标/通知分组关键，缺了显示空白图标）+ BrowserWindow icon 改用 app.ico（Windows 任务栏认 ICO）；**i18n**：`renderer/i18n.js` 共享词典（164 key，zh/en 平级，`data-i18n`/`data-i18n-ph` 静态替换 + `t()` 动态文案）、配置项 `language`（system/zh/en，设置页语言下拉）、主进程 `MAIN_TEXTS` 管托盘/通知/窗口标题（**日志保持中文**属技术输出）；`tools/check-i18n.js` 做 key 交叉核对。

## 三、当前状态

- **项目位置**：`D:\DSHManager`（自包含：源码 + node_modules/electron 43.4.0 + 鲸鱼 assets）。
- **启动方式**：桌面「DSH 管理器」快捷方式（指向 `D:\DSHManager\node_modules\electron\dist\electron.exe`，参数为项目目录）；或在该目录 `npm start`。
- **dsh 版本**：0.1.0-rc.7（2026-08-17 发布；npm 全局安装，前缀用 `npm prefix -g` 解析）。
- **DSH_HOME**：`~\.dsh`（默认；含凭据 `.env`、`settings.yaml`——默认模型 `deepseek-modlens / deepseek-v4-pro`、web profile 已装 `@liustack/modlens`）。
- **配置**：`%APPDATA%\DshManager\config.json`（模式见 ARCHITECTURE.md）。
- **图标工具**：`D:\DSHManager\tools\render-icon.ps1`（鲸鱼图标重新生成用，**必须纯 ASCII、STA 运行**）。

## 四、关键决策与原因（不要推翻，除非有明确理由）

| 决策 | 原因 |
|---|---|
| 用 Electron 而非继续 .NET | 用户明确要求；UI 用 Web 技术开发效率高 |
| 检测 dsh 用 **netstat 端口** 为主、WMI 命令行为辅 | WMI 路径匹配不可靠；netstat 在实测中稳定命中监听 3080 的进程 |
| 分析插件采用**管理器收集档案 → headless 判断** | headless profile 无联网工具（官方描述 "no Host, HTTP, or browser layer"）；且比代理自主浏览更省 token、更可控 |
| 分析结果用**结构化 JSON 评分卡** | Phase 0 实测模型能严格只输出一行 JSON（score/verdict/summary/pros/cons/risks） |
| 市场双源 npm + GitHub | 用户要求；GitHub 的 Star/Issues/活跃度本身就是"真伪"证据 |
| 更新前记录旧版本号、支持一键回滚 | npm 安装是覆盖式的，无内置回滚 |

## 五、遗留问题 / TODO

1. **旧工作区空目录**被受保护 crashpad 进程锁住删不掉——重启电脑后手动删除（已无文件，仅空壳）。
2. 未来可选：electron-builder 打包独立 exe（不依赖 node_modules）；分析结果横向对比/推荐榜；管理器自身更新（electron-updater）；GitHub Token 配置提限流（当前匿名限流：搜索 10 次/分钟、核心 60 次/小时，已做合并去重缓解）。
3. 分析功能每次消耗用户 API tokens（默认模型走 modlens），界面已有提示。
4. 旧工作区保留用户自己的实验项目（非管理器内容，勿动）。

## 六、新会话常用操作速查

- 改代码后**必须**杀旧实例再交付：`Get-Process electron | Stop-Process -Force`（单实例锁会让双击只唤醒旧代码窗口）。
- 代码在 `D:\DSHManager`（自包含；node_modules 需 `npm install`）。
- 语法检查：`node --check main.js`（preload.js、renderer/renderer.js、renderer/market.js 同理）。
- 修改任何 `.ps1`：**保持纯 ASCII**（见 GOTCHAS.md）。
- 受限环境（如 AI 沙箱）无法 GUI 实测 Electron 与 spawn 管道——不是代码问题，交付后请用户在真实环境验证。
- 发布公共仓库前：确认无个人路径/密钥（已清理）；推送用 `tools\publish.ps1`。
