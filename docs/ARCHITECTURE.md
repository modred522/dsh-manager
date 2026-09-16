# DSH 管理器 — 架构与代码地图

## 目录结构

```
dsh-manager
├── src-tauri/
│   ├── src/
│   │   ├── lib.rs           # 29 个 #[tauri::command]、窗口/托盘/定时器/启动编排、AppState
│   │   ├── config.rs        # config.json 读写（%APPDATA%\DshManager\），DSH_MANAGER_DATA 可重定向
│   │   ├── logging.rs       # 按天滚动日志（留 7 天，6 小时周期清理）、脱敏、子进程输出折叠
│   │   ├── pure.rs          # 纯函数：版本比较 / URL 归一化 / JSON 提取 / 日志脱敏
│   │   ├── gh.rs            # GitHub/HTTP 管道：客户端、令牌闸门、错误诊断、限流提示
│   │   ├── token.rs         # 可选 GitHub 令牌（存 Windows 凭据管理器，不落明文）
│   │   ├── dsh.rs           # npm / dsh 命令（Windows 上必须过 cmd.exe，见 GOTCHAS）
│   │   ├── procs.rs         # 进程探测：netstat2 取监听 PID + sysinfo 取详情与 CPU
│   │   ├── updates.rs       # dsh 版本查询 / changelog / 安装；管理器自身版本检查
│   │   ├── usage.rs         # Token 用量聚合（总量 / 按项目 / 按天）
│   │   ├── plugins.rs       # 已装插件管理（dsh plugin 转发 pnpm）
│   │   ├── market.rs        # 插件市场：游标分页搜索、npm/GitHub 详情
│   │   ├── analysis.rs      # 档案收集 + spawn dsh headless 评估 + 历史缓存
│   │   ├── shortcut.rs      # 桌面快捷方式（COM IShellLink；Tauri 无对应能力）
│   │   └── texts.rs         # 主进程文案（托盘/通知/窗口标题）中英
│   ├── capabilities/        # Tauri 2 权限。**必须自建**：没有这个目录，前端的
│   │                        # listen() 会被全部拒绝，而界面看着还能用（极具欺骗性）
│   ├── icons/ + tauri.conf.json + Cargo.toml + build.rs
├── renderer/                # 前端：原生 JS，无框架、无构建步骤
│   ├── index.html           # 三页签 + 2 个弹窗（更新/关于）
│   ├── renderer.js          # 主窗口界面逻辑
│   ├── i18n.js              # 中英词典（data-i18n + t()）——**三份脚本共享全局作用域**
│   ├── market.html/.js      # 插件市场独立窗口
│   ├── tauri-bridge.js      # 把 window.dsh.* 架在 invoke/listen 上（业务代码零改动）
│   └── styles.css           # CSS 变量 + 深色模式 + 全套动效
├── .github/workflows/       # check（cargo 三件套 + 渲染层自检）/ release（推 v* tag 发版）
├── assets/                  # whale.png、app.ico、social-preview.png、screenshots/
├── tools/                   # check-i18n / set-version / render-icon / social-preview / publish
└── docs/                    # MEMORY.md（交接）/ ARCHITECTURE.md / GOTCHAS.md
                             # TAURI-MIGRATION.md（Electron→Tauri 全过程与历次真机验收）
```
## lib.rs 的职责边界

模块划分见上面的目录结构。`lib.rs` 只留三类东西：

| 区块 | 职责 |
|---|---|
| 命令 | 29 个 `#[tauri::command]`。**建窗类命令必须是 `async`** —— 同步命令跑在事件循环所在的主线程上，而 `WebviewWindowBuilder::build()` 会把建窗任务投递给事件循环再阻塞等结果，于是互相等死（市场窗口白屏 + 关不掉就是这么来的） |
| 编排 | check / update / rollback 的流程（要动 `AppState` 与任务栏进度）、`send_state()`、`notify()`、`check_manager_update()` |
| 外壳 | 窗口（含窗口记忆）、托盘菜单（动态进程数）、主题、自启、全局快捷键 `Ctrl+Alt+D`、三个定时器（状态 6s/30s、自动检查小时级、看门狗 8s）、启动序列 |

`AppState`：`config` / `npm_prefix` / `latest_version` / `last_check_time` / `busy` / `core_packages`，全部 `Mutex` 包裹。
## 命令契约（renderer ↔ Rust，共 29 个）

渲染层调的始终是 `window.dsh.*`。Electron 时代由 `preload.js` 注入，现在由
`renderer/tauri-bridge.js` 架在 `invoke` / `listen` 上 —— **业务代码一行没改**。
通道命名各随其后端惯例：kebab-case（`get-state`）对 snake_case（`get_state`），
映射只发生在桥接层这一处。

| `window.dsh` 方法 | Tauri 命令 | 说明 |
|---|---|---|
| getState | get_state | 全量状态（版本/进程列表/busy/config/appInfo/rollbackVersion） |
| openDsh / restartDsh | open_dsh / restart_dsh | 启动并开浏览器 / 停止后重启 |
| stopDsh(pids?) | stop_dsh | 传数组停单个，不传停全部 |
| checkUpdates(silent) / update / rollback | 同名 | 检查（带 changelog）/ 更新 / 回滚 |
| getChangelog / getRecentLogs | 同名 | 更新日志 / 历史日志回填 |
| openConfigDir / openNpmDir / exportLog / createShortcut / setConfig | 同名 | 工具与设置 |
| getUsage / getPlugins / checkPluginUpdates / installPlugin / removePlugin / upgradePlugin | 同名 | 用量与插件 |
| marketSearch / pluginInfo / githubPluginInfo / installGithubPlugin | 同名 | 市场 |
| openMarket / openExternal | 同名 | 打开市场独立窗口 / 用系统浏览器打开链接 |
| pluginAnalyze(source,ref,force) / pluginAnalyzeStop / analysisHistory | 同名 | 分析（invoke 等完成；进度走事件） |
| getTokenStatus / setGithubToken / clearGithubToken | 同名 | 可选 GitHub 令牌（**只回传掩码，不回传完整令牌**） |
| logFrontendError（桥接层内部用） | log_frontend_error | 渲染层未捕获的 rejection 回传后端日志文件 |
| onLog / onState / onAnalyzeLog / onAnalyzeDone | 事件（Rust→renderer） | 日志 / 状态 / 分析流式输出 / 分析结果 |

> 事件需要 `capabilities/default.json` 授予 `core:event:default`。**自定义命令不受 ACL 管，
> 事件受**——所以权限漏配的表现是"点按钮有反应、实时日志和状态轮询全死"，
> 界面看着完全正常。这也是 `log_frontend_error` 走 invoke 而不走事件的原因。
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
  "updateChannel": "all", "autoUpdateManager": true,
  "cleanAnalysisSessions": true, "language": "system",
  "windowBounds": {"x":..,"y":..,"width":..,"height":..},
  "rollbackVersion": null
}
```

## 依赖的 dsh 环境（本机事实）

- npm 前缀：`npm prefix -g` 自动解析（如 `%LOCALAPPDATA%\npm-global`）；dsh 本体 `node_modules\@deepseek-ai\dsh\lib\bin.js`。
- `DSH_HOME = ~\.dsh`（默认）：`settings.yaml`（默认模型 deepseek-modlens/deepseek-v4-pro）、`.env`（DEEPSEEK_API_KEY）、`profiles\web\`（dependencies + dsh.profile.bundles + pnpm-workspace.yaml）、`sessions\`、`storages\session_projcache.json`、`attachments\`。
- 命令：`dsh web`（3080 端口）、`dsh --profile headless "<任务>"`（一次性）、`dsh plugin --profile <p> add|remove <pkg>`（转发 pnpm）。

## 各功能实现要点

- **dsh 进程检测**：`netstat2` crate 取监听端口的 PID（IPv4 + IPv6 都查）；`sysinfo` 补命令行/启动时间/内存/CPU（CPU% 按两次采样差计算）。原先是 spawn `netstat -ano` 解析文本 + `find-dsh.ps1` 走 Get-CimInstance，现在都不再起子进程。命令行的双反斜杠（npm shim `%dp0%\` 拼接产生）显示前归一化。
- **用量仪表盘**：读 `session_projcache.json` 的 `tables.sessions[*]`：`identity.cwd/createdAt`、`rows.title.val`、`rows.tokenUsage.val.totals`（uncachedInputTokens/cacheReadTokens/cacheWriteTokens/outputTokens）、`rows.sessionListMetadata.val.lastPromptAt`（**曾误写成 `listMeta`**，导致趋势图按创建时间而非活动时间分桶）。按 cwd 聚合项目、按天聚合近 14 天；费用 = tokens/1e6 × 单价（单价在 UI 可改）。
- **市场搜索**：市场是**独立窗口**（`market.html`，主窗口插件页按钮经 open-market 打开，重复点击聚焦已有窗口），卡片上直接展示 GitHub 页面链接（npm 源无仓库信息时回退 npm 页面），点击经 open-external 用系统浏览器打开。数据源：npm `/-/v1/search?text=...&size=25&from=<offset>`（**默认相关度排序**——registry 内部综合 质量/维护/流行度 打分；关键词 queries 合并去重；排除 dsh CLI 自身依赖即核心包；官方 = @deepseek-ai/ scope；`links.repository/homepage` 归一化为 GitHub 页面 URL）、GitHub `search/repositories`（`sort=stars` **星标降序**，`page=<n>` 翻页，结果自带 `html_url`）。**分页**：`searchMarketPage()` 维护单一搜索游标（seen 去重 + 每 query 的 offset/page，源或关键词变化即重置），每次返回约 20 条 `{items,hasMore,rateLimited}`；渲染层滑到底部自动 `loadMore()` 追加、底部状态条显示"加载更多/已加载全部"，限流时提示并可点击重试。**GitHub 未登录接口按出口 IP 限额**（搜索 10 次/分钟、其余 60 次/小时），公司 NAT 下这份额度是全办公室共用的 —— 设置里配一个 GitHub 令牌可提到 5000 次/小时（存凭据管理器，见 `token.rs`）。上限防御：npm offset<500、GitHub 页≤50。
- **插件详情视图**：市场窗口内整页视图（← 返回市场切换），README（原始数据）左栏、评分卡 + 分析控制台右栏上下分栏，**分割条可拖拽调整大小**（localStorage 记忆比例、双击复位），空状态用 `:empty::before` 占位。
- **分析管线**：收集档案（npm 元数据+周下载+README 15KB+GitHub 仓库活跃度；GitHub 源则仓库统计+package.json+README）→ `ensureAnalysisEnv`（对比 web/headless profile 的 bundles，自动 `dsh plugin --profile headless add <缺失插件>`）→ spawn `node <bin.js> --profile headless --patch <补丁> <提示词>`（10 分钟超时/可停止/流式）→ 从输出提取最后可解析 JSON → 评分卡 + 历史缓存 `%APPDATA%\DshManager\analyses\`。**会话隔离**：`ensureHeadlessSessionPatch()` 生成 `%APPDATA%\DshManager\headless-session-patch.yml`，用 dsh 官方 `--patch` 层把 `session-persistence-jsonl.root` 重定向到 `%APPDATA%\DshManager\analysis-sessions\`（web 列表/用量统计都看不到分析会话）；分析结束后按 `cleanAnalysisSessions`（默认开）清理该私有目录，绝不触碰 `$DSH_HOME/sessions` 下用户会话；`verifyHeadlessPatchRow()` 在启动时校验当前 dsh 版本仍有该行 id，缺失则日志告警。
- **GitHub 安装**：渲染层先弹供应链风险确认（prepare 构建脚本），确认后 `ensureAllowBuilds` 把包名写进 `profiles\web\pnpm-workspace.yaml` 的 `allowBuilds`，再 `dsh plugin --profile web add github:owner/repo`。
- **主题**：Rust 侧设窗口 theme；renderer 按 config.theme 给 body 加 `dark` class（CSS 变量驱动，鲸鱼 Logo 深色下 invert）。

## 扩展指南（加新功能的最小路径）

1. 对应模块加函数 → `lib.rs` 加 `#[tauri::command]` 并注册进 `generate_handler!` → `tauri-bridge.js` 暴露方法 → renderer 页面/弹窗 + 样式 + 事件绑定。
2. 记得同步维护：`busy` 状态（渲染层按钮禁用）、`send_state` 字段、深色模式颜色变量、i18n 中英两份词条。
3. 新增的是**建窗命令**就必须写成 `async`（见上面 lib.rs 那节），否则主线程自锁。
4. 新增**事件**要确认 `capabilities/default.json` 的权限覆盖到；改了 capabilities 之后
   `touch src-tauri/build.rs` 强制 build script 重跑，否则产物里还是旧 ACL。
5. 交付前跑：`cargo fmt --all --check` + `cargo clippy --all-targets -- -D warnings` +
   `cargo test` + `node tools/check-i18n.js` + `npx tauri build` + 隔离启动烟测。
