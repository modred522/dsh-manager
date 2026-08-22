# DSH 管理器 — 架构与代码地图

## 目录结构

```
dsh-manager
├── main.js                  # 主进程（约 1500 行：入口 + 窗口/托盘 + 进程检测 + 更新 + 用量 + 插件安装/分析 + IPC 装配）
├── preload.js               # contextBridge 暴露 window.dsh（32 个 IPC 方法）
├── find-dsh.ps1             # WMI 进程详情脚本（必须纯 ASCII）
├── lib/
│   ├── pure.js              # 纯函数：版本比较/URL 归一化/JSON 提取/文案转换（无副作用，可测）
│   └── market.js            # 插件市场搜索与详情（只读、纯网络）
├── test/
│   └── pure.test.js         # node:test 单元测试（`npm test`）
├── package.json             # electron 43.4.0；npm start = electron .；npm test = node --test
├── electron-builder.yml     # 打包配置（NSIS + zip + GitHub publish）
├── .github/workflows/       # check（语法+单测）/ release（tag 自动发版）
├── assets/                  # whale.png、app.ico、social-preview.png、screenshots/
├── tools/                   # render-icon / social-preview / build-release / publish / check-i18n
├── renderer/
│   ├── index.html           # 三页签 + 2 个弹窗（更新/关于）
│   ├── renderer.js          # 主窗口界面逻辑
│   ├── i18n.js              # 中英词典（data-i18n + t()）
│   ├── market.html          # 插件市场独立窗口页面
│   ├── market.js            # 插件市场窗口逻辑（搜索/详情/分析/安装）
│   └── styles.css           # CSS 变量 + 深色模式 + 全套动效
└── node_modules/            # 本地安装的 electron 等
```

## main.js 模块划分（按出现顺序）

| 区块 | 职责 |
|---|---|
| 错误兜底 | uncaughtException/unhandledRejection → crash.log；`DSH_USER_DATA` 环境变量可重定向 userData（测试用） |
| DEFAULT_CONFIG | 配置默认值（含费用单价、watchdog、theme、windowBounds、rollbackVersion） |
| 配置读写 | config.json 读写（`%APPDATA%\DshManager\`） |
| 日志 | `log()` 写滚动日志文件（logs\ 按天、保留 7 天）+ 广播到主窗口与市场窗口；`getRecentLogLines()` 供重启后回填 |
| npm/DSH 交互 | resolveNpmPrefix、getInstalledVersion、getLatestVersion、compareVersions、isServerUp、openBrowser、launchDsh、isDshRunning、dshPort |
| 进程检测 | `netstatListeningPids`（Node 直连 netstat，每次扫描）、`wmiProcessDetails`（find-dsh.ps1，30 秒缓存，含 CPU/内存采样）、`getDshProcesses`（合并）、`stopDshProcesses(pids)`（支持单个/全部）、`restartDsh`、`checkWatchdog`（8 秒守护） |
| 更新 | `runInstall(version)`、`performInstall`（记录运行态→停进程→任务栏进度→恢复）、`update`（记录 rollbackVersion+自动重启）、`rollback` |
| 更新日志 | `fetchChangelog`（GitHub Releases，只保留中文段）、`releaseBodyToText` |
| 用量统计 | `getUsage()`（读 session_projcache.json 聚合 总量/按项目/按天） |
| 插件管理 | `getProfilePlugins`（web profile 的 package.json dependencies）、`runPluginCommand`（dsh plugin 转发 pnpm）、install/removePlugin |
| 市场 | `searchMarketPage`（游标式分页：多关键词合并去重、排除核心包、限流标记）、`getNpmPluginInfo`、`getGithubPluginInfo`、`installGithubPlugin`（写 allowBuilds） |
| 分析 | `ANALYSIS_PROMPT`（Phase 0 验证过的模板）、`ensureAnalysisEnv`（自动补装 headless 缺失插件）、`runHeadlessAnalysis`（直接 spawn node + bin.js，10 分钟超时）、`extractAnalysisJson`、分析历史缓存（analyses\） |
| 状态推送 | `sendState()`（含进程列表/用量无关字段，**广播**到主窗口 + 市场窗口）、`broadcast()`、`notify`、`appInfo` |
| 窗口/托盘 | createWindow（窗口记忆）、**createMarketWindow（插件市场独立窗口）**、buildTrayMenu/updateTray（动态进程数）、showWindow |
| 定时器 | stateTimer（可见 6s / 隐藏 30s）、autoCheckTimer（小时级）、watchdogTimer（8s） |
| IPC | setupIpc() 集中注册所有 handler |
| 启动 | 单实例锁 → loadConfig → resolveNpmPrefix → loadCorePackages → 窗口/托盘/定时器/快捷键（Ctrl+Alt+D） |

## IPC 契约（preload ↔ main，共 27 个）

| preload 方法 | IPC 通道 | 说明 |
|---|---|---|
| getState | get-state | 全量状态（版本/进程列表/busy/config/appInfo/rollbackVersion） |
| openDsh / restartDsh | open-dsh / restart-dsh | 启动并开浏览器 / 停止后重启 |
| stopDsh(pids?) | stop-dsh | 传数组停单个，不传停全部 |
| checkUpdates(silent) / update / rollback | 同名通道 | 检查（带 changelog）/ 更新 / 回滚 |
| getChangelog / getRecentLogs | 同名通道 | 更新日志 / 历史日志回填 |
| openConfigDir / openNpmDir / exportLog / createShortcut / setConfig | 同名通道 | 工具与设置 |
| getUsage / getPlugins / installPlugin / removePlugin | 同名通道 | 用量与插件 |
| marketSearch / pluginInfo / githubPluginInfo / installGithubPlugin | 同名通道 | 市场 |
| openMarket / openExternal | open-market / open-external | 打开插件市场独立窗口 / 用系统浏览器打开 http(s) 链接 |
| pluginAnalyze(source,ref,force) / pluginAnalyzeStop / analysisHistory | 同名通道 | 分析（invoke 等待完成；进度走事件） |
| onLog / onState / onAnalyzeLog / onAnalyzeDone | 事件（main→renderer） | 日志 / 状态 / 分析流式输出 / 分析结果 |

## state 结构（get-state / state 事件）

```js
{
  installedVersion, latestVersion, dshUrl, serverUp, dshRunning,
  dshProcesses: [{ pid, listening, name, commandLine, startTime, memMb, cpuPercent }],
  busy: null|'check'|'update'|'plugin'|'analyze',
  lastCheckTime, rollbackVersion, config, appInfo
}
```

## config.json 模式（%APPDATA%\DshManager\config.json）

```json
{
  "dshUrl": "http://127.0.0.1:3080",
  "autoCheckOnStartup": true, "autoCheckIntervalHours": 6,
  "autoStartWithWindows": false, "createDesktopShortcut": true,
  "minimizeToTrayOnStartup": false,
  "costInput": 2, "costCache": 0.5, "costOutput": 8,
  "watchdog": true, "theme": "system",
  "cleanAnalysisSessions": true,
  "windowBounds": {"x":..,"y":..,"width":..,"height":..},
  "rollbackVersion": null
}
```

## 依赖的 dsh 环境（本机事实）

- npm 前缀：`npm prefix -g` 自动解析（如 `%LOCALAPPDATA%\npm-global`）；dsh 本体 `node_modules\@deepseek-ai\dsh\lib\bin.js`。
- `DSH_HOME = ~\.dsh`（默认）：`settings.yaml`（默认模型 deepseek-modlens/deepseek-v4-pro）、`.env`（DEEPSEEK_API_KEY）、`profiles\web\`（dependencies + dsh.profile.bundles + pnpm-workspace.yaml）、`sessions\`、`storages\session_projcache.json`、`attachments\`。
- 命令：`dsh web`（3080 端口）、`dsh --profile headless "<任务>"`（一次性）、`dsh plugin --profile <p> add|remove <pkg>`（转发 pnpm）。

## 各功能实现要点

- **dsh 进程检测**：Node spawn `netstat -ano` 解析 `LISTENING` 行取 PID（可靠）；find-dsh.ps1 用 Get-CimInstance 补命令行/启动时间/内存/CPU 时间（30 秒缓存，CPU% 按两次采样差计算）。命令行的双反斜杠（npm shim `%dp0%\` 拼接产生）显示前归一化。
- **用量仪表盘**：读 `session_projcache.json` 的 `tables.sessions[*]`：`identity.cwd/createdAt`、`rows.title.val`、`rows.tokenUsage.val.totals`（uncachedInputTokens/cacheReadTokens/cacheWriteTokens/outputTokens）、`rows.listMeta.val.lastPromptAt`。按 cwd 聚合项目、按天聚合近 14 天；费用 = tokens/1e6 × 单价（单价在 UI 可改）。
- **市场搜索**：市场是**独立窗口**（`market.html`，主窗口插件页按钮经 open-market 打开，重复点击聚焦已有窗口），卡片上直接展示 GitHub 页面链接（npm 源无仓库信息时回退 npm 页面），点击经 open-external 用系统浏览器打开。数据源：npm `/-/v1/search?text=...&size=25&from=<offset>`（**默认相关度排序**——registry 内部综合 质量/维护/流行度 打分；关键词 queries 合并去重；排除 dsh CLI 自身依赖即核心包；官方 = @deepseek-ai/ scope；`links.repository/homepage` 归一化为 GitHub 页面 URL）、GitHub `search/repositories`（`sort=stars` **星标降序**，`page=<n>` 翻页，结果自带 `html_url`）。**分页**：`searchMarketPage()` 维护单一搜索游标（seen 去重 + 每 query 的 offset/page，源或关键词变化即重置），每次返回约 20 条 `{items,hasMore,rateLimited}`；渲染层滑到底部自动 `loadMore()` 追加、底部状态条显示"加载更多/已加载全部"，限流时提示并可点击重试（GitHub 匿名限流 10 次/分钟；上限防御：npm offset<500、GitHub 页≤50）。
- **插件详情视图**：市场窗口内整页视图（← 返回市场切换），README（原始数据）左栏、评分卡 + 分析控制台右栏上下分栏，**分割条可拖拽调整大小**（localStorage 记忆比例、双击复位），空状态用 `:empty::before` 占位。
- **分析管线**：收集档案（npm 元数据+周下载+README 15KB+GitHub 仓库活跃度；GitHub 源则仓库统计+package.json+README）→ `ensureAnalysisEnv`（对比 web/headless profile 的 bundles，自动 `dsh plugin --profile headless add <缺失插件>`）→ spawn `node <bin.js> --profile headless --patch <补丁> <提示词>`（10 分钟超时/可停止/流式）→ 从输出提取最后可解析 JSON → 评分卡 + 历史缓存 `%APPDATA%\DshManager\analyses\`。**会话隔离**：`ensureHeadlessSessionPatch()` 生成 `%APPDATA%\DshManager\headless-session-patch.yml`，用 dsh 官方 `--patch` 层把 `session-persistence-jsonl.root` 重定向到 `%APPDATA%\DshManager\analysis-sessions\`（web 列表/用量统计都看不到分析会话）；分析结束后按 `cleanAnalysisSessions`（默认开）清理该私有目录，绝不触碰 `$DSH_HOME/sessions` 下用户会话；`verifyHeadlessPatchRow()` 在启动时校验当前 dsh 版本仍有该行 id，缺失则日志告警。
- **GitHub 安装**：渲染层先弹供应链风险确认（prepare 构建脚本），确认后 `ensureAllowBuilds` 把包名写进 `profiles\web\pnpm-workspace.yaml` 的 `allowBuilds`，再 `dsh plugin --profile web add github:owner/repo`。
- **主题**：main 设 `nativeTheme.themeSource`；renderer 按 config.theme 给 body 加 `dark` class（CSS 变量驱动，鲸鱼 Logo 深色下 invert）。

## 扩展指南（加新功能的最小路径）

1. main.js 加函数 → setupIpc() 注册 handler → preload.js 暴露方法 → renderer 页面/弹窗 + 样式 + 事件绑定。
2. 记得同步维护：`busy` 状态（渲染层按钮禁用）、`sendState` 字段、深色模式颜色变量。
3. 交付前跑：`node --check` 三个 JS + id 交叉核对 + （如有 .ps1）纯 ASCII 检查 + 杀旧 electron 实例。
