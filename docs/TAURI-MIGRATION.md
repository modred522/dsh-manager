# DSH 管理器 — Electron → Tauri 2 重构方案（待评审）

> 状态：**阶段一、阶段二已落地**（分支 `tauri`）。第七节 6 个决策按推荐默认值执行，如需改动请直说。
> 所有 API 名称已对照 Tauri 2 官方文档核实。

## 一、结论

可行，且收益明确。本机工具链已就绪（rustc/cargo 1.98、WebView2 Runtime 152）。

| | Electron v1.0.6（实测） | Tauri 阶段一（实测） | 缩减 |
|---|---|---|---|
| NSIS 安装器 | 95.6 MB | **2.1 MB** | 46× |
| 主程序 exe | 215.1 MB | **5.7 MB** | 38× |
| 安装后占用 | 349.6 MB | **5.7 MB** | 61× |
| 常驻内存 | ~150–250 MB | 待实测（WebView2 由系统共享） | — |
| 运行时依赖 | 自带 Chromium | 系统 WebView2（Win11 自带；Win10 需引导安装） | — |

> 体积是阶段一产物的**实测值**（原先估的是 ~10–15 MB 安装器，估保守了）。Tauri 没有
> zip 打包目标，便携分发就是那个 5.7 MB 的 exe 本身。

**但要说清楚：这不是"移植"，是后端重写。** 前端能原样搬，主进程那 2066 行 JS 得变成 Rust。

## 二、工作量拆分

| 部分 | 行数 | 处理方式 |
|---|---|---|
| `renderer/*.html` `styles.css` | ~1800 | **原样搬**，零改动 |
| `renderer/renderer.js` `market.js` `i18n.js` | ~1400 | **改调用层**：`window.dsh.x()` → `invoke('x')`，`onLog/onState` → `listen()`。业务逻辑不动 |
| `main.js` | 1698 | **Rust 重写** |
| `lib/pure.js` | 167 | **Rust 重写**（纯函数，13 条单测可 1:1 翻成 `#[test]`） |
| `lib/market.js` | 201 | **Rust 重写**（reqwest） |
| `find-dsh.ps1` | 41 | **可以直接删掉**（见下） |

Rust 侧预计 2500–3500 行（类型和错误处理会比 JS 啰嗦）。

## 三、API 映射

全部是官方插件，crate 名已核实：

| 现在用的 | Tauri 2 对应 |
|---|---|
| `Tray` + `Menu` | `tauri::tray::TrayIconBuilder`（**核心内置**，不是插件） |
| `app.requestSingleInstanceLock` | `tauri-plugin-single-instance` |
| `globalShortcut`（Ctrl+Alt+D） | `tauri-plugin-global-shortcut` |
| `app.setLoginItemSettings` | `tauri-plugin-autostart` |
| `Notification` | `tauri-plugin-notification` |
| `shell.openExternal` / `openPath` | `tauri-plugin-opener` |
| `spawn`（dsh / npm / taskkill） | `tauri-plugin-shell` 或 Rust `std::process` |
| `dialog`（导出日志另存为） | `tauri-plugin-dialog` |
| `config.windowBounds` + `isBoundsVisible` | `tauri-plugin-window-state`（**这块自研逻辑可整段删掉**） |
| `electron-updater` | `tauri-plugin-updater`（⚠️ 见决策 4） |
| `mainWindow.setProgressBar(2)` | `WebviewWindow::set_progress_bar(ProgressBarState)` ✅ 已确认 Tauri 2 支持 |
| `nativeTheme.themeSource` | `WebviewWindow::set_theme()` |
| `http.get`（探活）+ `fetch`（市场） | `reqwest` |
| `app.getPath('appData')` | `tauri::path::BaseDirectory::AppData` |
| `shell.writeShortcutLink` | ❌ **无对应，要自己写**（见硬骨头 1） |

## 四、29 个 IPC 通道怎么接

现有 29 个 `invoke` + 4 个事件。绝大多数是机械翻译：

- **21 个直接 → `#[tauri::command]`**：`get-state` `open-dsh` `restart-dsh` `stop-dsh` `check-updates` `update` `rollback` `get-changelog` `get-recent-logs` `open-config-dir` `open-npm-dir` `get-usage` `get-plugins` `check-plugin-updates` `install-plugin` `remove-plugin` `upgrade-plugin` `market-search` `plugin-info` `github-plugin-info` `analysis-history`
- **4 个走插件**：`open-external`（opener）、`export-log`（dialog + fs）、`create-shortcut`（自研）、`set-config`（自研 + autostart/theme 联动）
- **2 个窗口管理**：`open-market`（多窗口，Tauri 原生支持）、`install-github-plugin`
- **2 个长任务**：`plugin-analyze` / `plugin-analyze-stop` —— 用 `tauri::async_runtime::spawn` + 句柄存进 `State`，`stop` 时 kill
- **4 个事件**：`log` `state` `analyze-log` `analyze-done` → `app.emit()`，前端 `listen()`。现在的 `broadcast()`（主窗口 + 市场窗口）用 `app.emit()` 天然就是全窗口广播

**顺带修掉一个历史包袱**：GOTCHAS 4.8 说 `compareVersions` 在 `main.js` 和 `renderer.js` 各有一份、改动要两处同步。Tauri 下前端可以直接 `invoke('compare_versions')`，**这份重复可以彻底消掉**。

## 五、五块硬骨头

### 1. 桌面快捷方式（唯一真正没现成方案的）
`shell.writeShortcutLink` 没有 Tauri 等价物。两条路：
- **A（推荐）**：Rust 调 COM `IShellLink` + `IPersistFile`（`windows` crate）。约 40 行 unsafe，一次写好不用再碰。
- **B**：保留一个 PowerShell 辅助脚本（`WScript.Shell.CreateShortcut`）。省事，但把 GOTCHAS 第一节那堆 PS 编码坑又带进来了。

### 2. 进程探测（反而变简单）
现在是 `spawn netstat -ano` 解析文本 + `find-dsh.ps1` 走 WMI。Rust 下可以直接调 Windows API：
- 监听端口 → `GetExtendedTcpTable`（`windows` crate），不用起子进程、不用解析文本
- 进程详情（命令行/启动时间/内存/CPU）→ `wmi` crate，或 `NtQueryInformationProcess`
- **`find-dsh.ps1` 可以整个删掉，GOTCHAS 第一节那堆 PowerShell 纯 ASCII 的坑一起消失**

### 3. 子进程流式输出
`launchDsh` / `runInstall` / `runPluginCommand` / `runHeadlessAnalysis` 都是 spawn + 逐块 stdout → 广播。Tauri 侧：`tauri_plugin_shell::Command::spawn()` 返回 `CommandChild` + 事件流，或 `tokio::process` 配 `BufReader::lines()`。我倾向 tokio 直接管，对超时/kill 的控制更顺手（分析那条要 10 分钟超时 + 可中断）。
刚修的 `logChildOutput`（拆行 + 重复折叠 + 限频）逻辑照搬。

### 4. 市场分页游标
`searchMarketPage` 维护一个跨调用的可变状态（`seen` 去重集合 + 每 query 的 offset/page，源或关键词变化即重置）。Rust 下放 `State<Mutex<MarketState>>`。直译即可，注意 `async` 下别把 `MutexGuard` 跨 `await` 持有（用 `tokio::sync::Mutex` 或先取值再释放）。

### 5. 分析管线
最绕的一块，但没有 Tauri 特有的坑：
- `ensureHeadlessSessionPatch()` 写 YAML 补丁 → `serde_yaml`，或就按现在的方式拼字符串（只有 5 行，拼字符串更稳）
- `ensureAnalysisEnv()` 比对 web/headless profile 的 bundles → 读 `package.json`，`serde_json`
- `ensureAllowBuilds()` 改 `pnpm-workspace.yaml` → **这里必须用 `serde_yaml` 保守改写**，别用正则
- `spawn node <bin.js> --profile headless --patch <file> <prompt>` → tokio，10 分钟超时
- **会话隔离那条铁律照搬**：只删管理器自己的 `analysis-sessions`，绝不碰 `$DSH_HOME/sessions`

## 六、分四阶段落地

1. **骨架 + 主线**（能跑起来看得见）：Tauri 2 项目、renderer 搬过去、`get-state` / `open-dsh` / `stop-dsh` / `restart-dsh` / 托盘 / 单实例 / 日志事件。产出：能启停 DSH 的托盘应用 + 体积对比数据。
2. **更新与设置**：`check-updates` / `update` / `rollback` / changelog / `set-config` / 主题 / 自启 / 全局快捷键 / 快捷方式（硬骨头 1）/ 任务栏进度。
3. **用量与插件**：`get-usage` 聚合、插件增删升、`check-plugin-updates`。
4. **市场与分析**：市场独立窗口 + 分页 + 双源、分析管线 + 评分卡 + 历史缓存。

每阶段结束都跑一次"真打包 + 装上真启动"（见第八节）。

## 七、要你拍的决策

1. **前端要不要换框架？** 建议**不换**——现有 3248 行原生 HTML/CSS/JS 能省下的工作量很实在，i18n 那套 175 key 的字典也是现成的。上 Vite/Svelte 会多出构建链和一轮返工。
2. **硬骨头 1 走 COM 还是 PowerShell？** 建议 COM（一次性代价，换掉一类长期坑）。
3. **配置目录保持 `%APPDATA%\DshManager\`？** 建议**保持**，这样老用户的 `config.json`、7 天日志、`analyses/` 历史缓存全部平滑继承。代价是要兼容读现有 JSON 结构（反正 serde 很容易）。
4. **⚠️ 自动更新的断点怎么处理？** 这条最需要你决定：`electron-updater`（`latest.yml`）和 `tauri-plugin-updater`（`latest.json` + **强制 Ed25519 签名**，不可关闭）互不相容。**现有 v1.0.x 用户没法自动更新到 Tauri 版**。选项：(a) 最后发一版 Electron，把更新提示改成"请手动下载新版"；(b) 保持 NSIS 的 appId/productName 不变，让 Tauri 安装器能原地覆盖安装，用户手动装一次即可。建议 a+b 都做。
5. **仓库怎么放？** 建议同仓库新分支 `tauri`，`src-tauri/` 与现有 `main.js` 并存一段时间，第四阶段完成后再删 Electron 侧。这样随时能回退、也能对照。
6. **代码签名？** 现状是 `CSC_LINK`/`CSC_KEY_PASSWORD` 走 GitHub Secrets（证书需自购）。Tauri 侧同样支持，但**更新签名（Ed25519）和代码签名（Authenticode）是两件事**，前者必须配。

## 八、验证策略（v1.0.5 的教训要带过来）

那次事故的根因是"`node --check` 过了就以为能发"。Tauri 侧对应的陷阱是 `tauri.conf.json` 的 `bundle.resources` / 前端 `frontendDist` 漏配——**`cargo build` 全绿，装上照样找不到文件**。

所以从第一阶段就把这三道关立起来：

- `cargo test`（`lib/pure.js` 那 13 条单测翻成 Rust `#[test]`）
- `cargo clippy -- -D warnings`
- **每阶段一次 `tauri build` + 装上真启动一次**，别只看 `tauri dev`

`tools/check-package.js` 对 Tauri 不适用（Rust 静态链接，没有相对 require 这回事），但它替代的那个动作——**对产物本身做核对，而不是对源码**——必须保留。

---

## 九、阶段一落地记录（分支 `tauri`）

### 已经能用

| 能力 | 实现位置 |
|---|---|
| 配置读写（**沿用 `%APPDATA%\DshManager\config.json`**，camelCase 键不变） | `src-tauri/src/config.rs` |
| 日志：按天滚动、7 天保留、token 打码、子进程输出节流 | `src-tauri/src/logging.rs` |
| 纯函数：版本比较 / changelog 清洗 / 评分卡提取 / 打码 | `src-tauri/src/pure.rs` |
| 进程探测：监听端口 + 进程详情（名称/命令行/内存/CPU） | `src-tauri/src/procs.rs` |
| dsh 交互：npm 前缀、已装版本、HTTP 探活、启动并流式转发输出 | `src-tauri/src/dsh.rs` |
| 托盘、单实例、窗口记忆、主题、状态轮询、崩溃守护 | `src-tauri/src/lib.rs` |
| 托盘/标题中英文案 | `src-tauri/src/texts.rs` |

已接通的命令：`get_state` `open_dsh` `stop_dsh` `restart_dsh` `get_recent_logs`
`set_config` `open_config_dir` `open_npm_dir` `open_external` `open_market`。
其余 19 个命令已注册为占位，调用会返回「该功能正在迁移到 Tauri 版，尚未实现」。

### 渲染层真的没改业务逻辑

新增 `renderer/tauri-bridge.js`：把 `window.dsh.*` 这套契约架在 Tauri 的 `invoke`/`listen` 上。
它在两种情况下自动让路——`window.dsh` 已存在（Electron 的 preload 注入过）或没有 `window.__TAURI__`——
所以**同一份 renderer 能同时跑在两种后端上**，`renderer.js` / `market.js` 的业务代码一行没动。

两处顺带调整（两种后端都受益）：

* About 弹窗的运行时信息改由后端给出名称（Electron 给 `Electron/Node.js/Chromium`，
  Tauri 给 `Tauri/Rust/WebView2`），顺手删掉 3 个因此失效的 i18n key。
* HTML 的 CSP 加了 `connect-src 'self' ipc: http://ipc.localhost`（Tauri IPC 需要，Electron 不受影响）。

### 比原方案多赚的

进程探测没有按原计划手写 `GetExtendedTcpTable` 的 unsafe FFI，改用 `netstat2` + `sysinfo`：
两套地址族（IPv4/IPv6）都覆盖、CPU% 由 `sysinfo` 自己采样，比手算 100ns 差值和写两份 unsafe 稳得多。
**这意味着 `find-dsh.ps1` 在 Electron 侧退场后即可删除**。

### 移植中被单测抓出来的真 bug

`Command::new("npm")` 在 Windows 上会报找不到程序——`npm` 实际是 `npm.cmd`，而 Rust 的
`Command` 不像 cmd.exe 那样按 PATHEXT 补后缀。Electron 版用 Node 的 `exec()` 过 shell 所以从没撞上。
如果没有那条「npm 前缀应能解析」的单测，这个会变成运行期静默失效：版本显示不出来、插件功能全废。
现在所有 npm/dsh 调用统一走 `dsh::shell_command()`，并留了一条回归测试。

### 阶段一验证结果

* `cargo test` — **39 passed / 0 failed**（含从 `test/pure.test.js` 1:1 翻过来的 16 条）
* `cargo check --all-targets` — 0 error / 0 warning
* `cargo clippy --all-targets -- -D warnings` — 见提交说明

### 还没做（按阶段推进）

阶段二：更新检查/更新/回滚/changelog、通知、全局快捷键 `Ctrl+Alt+D`、开机自启、
桌面快捷方式（硬骨头 1，走 COM）、任务栏进度。
阶段三：用量聚合、插件增删升。
阶段四：市场分页 + 双源、分析管线 + 评分卡 + 历史缓存。

打包（`tauri build`）与体积实测留到阶段二做——按第八节的规矩，每阶段都要真打包、真启动一次。

---

## 十、阶段二落地记录

### 本阶段新增的能力

| 能力 | 说明 |
|---|---|
| 检查更新 / 更新 / 回滚 | `updates.rs` 查 npm dist-tags、拉官方 changelog、`npm install -g` 流式装；编排在 `lib.rs` |
| 更新通道 | `all`（含预发布）/ `latest`（仅稳定），预发布版在日志与通知里都标注 |
| 系统通知 | `tauri-plugin-notification`，发现新版 / 更新完成 / 回滚完成三处 |
| 全局快捷键 | `tauri-plugin-global-shortcut`，`Ctrl+Alt+D` 唤起窗口并打开 DSH |
| 开机自启 | `tauri-plugin-autostart`，设置页开关即时生效 |
| **桌面快捷方式** | `shortcut.rs` 走 COM `IShellLink` + `IPersistFile`（硬骨头 1，按决策 2 选 COM） |
| 导出日志 | `tauri-plugin-dialog` 的保存对话框 |
| 任务栏进度 | 安装期间 `set_progress_bar(Indeterminate)` |
| 定时检查 | 按 `autoCheckIntervalHours` 起定时器，改设置立即换代生效 |
| 托盘菜单 | 补上「检查更新」「创建桌面快捷方式」 |
| 崩溃兜底 | panic hook 写 `crash.log`，落在配置目录而不是安装目录（后者在 Program Files 下可能没写权限） |

命令进度：**16 / 29 已接通**，剩 13 个（用量、插件、市场、分析）是阶段三、四的活。

### 硬骨头 1 的结论

COM 方案成立。`shortcut.rs` 约 60 行 unsafe，单测 `writes_a_real_lnk_file` 真写出一个 `.lnk`
并校验文件头魔数（`4C 00 00 00`）—— 验的不是能不能编译，是有没有生成有效的快捷方式。
于是 PowerShell 辅助脚本那条备选路彻底不需要，GOTCHAS 第一节那堆编码坑没被带进新架构。

### 加了一道启动烟测

`cargo test` 再全绿也不能证明 GUI 起得来（v1.0.5 的教训正在这里）。所以本阶段加了一道隔离启动烟测：
用 `DSH_MANAGER_DATA` 把管理器数据目录指到临时位置、配置里关掉建快捷方式与自启，
启动真实产物、确认进程活过启动阶段、检查有没有 `crash.log`，最后核对真实桌面与注册表没被动过。

这道烟测立刻还本了，抓到两个东西：

1. **一个真 bug**：自启本来就没开时调 `disable()`，插件会报「系统找不到指定的文件 (os error 2)」，
   每次启动都往日志里刷一条看着像坏了的错误。已改成先查 `is_enabled()`、只在需要改变时才动手。
2. **一个我自己挖的坑**：最初用重定向 `APPDATA` 来隔离，结果 `npm prefix -g` 跟着变了
   （npm 的全局前缀默认就在 `%APPDATA%\npm`），管理器找不到已装的 dsh，报「无法获取版本信息」。
   看着像产品 bug，其实是测试手法错了。为此给应用加了 `DSH_MANAGER_DATA` 覆盖变量
   （只隔离管理器自己的数据，不碰 npm），这条教训也写进了 `config.rs` 的注释。

### 阶段二验证结果

- `cargo test` — **49 passed / 0 failed**
- `cargo clippy --all-targets -- -D warnings` — 干净
- `cargo fmt --check` — 干净
- `tauri build` — NSIS 安装器产出正常
- 启动烟测 — 进程存活、无 `crash.log`、真实桌面与注册表未被改动
- 常驻内存实测 **约 40 MB**（Electron 版 150–250 MB）

### 还没做

阶段三：用量聚合（读 `session_projcache.json`）、插件增删升、插件更新检查。
阶段四：市场独立窗口 + 双源分页、分析管线 + 评分卡 + 历史缓存。

另外**决策 4（updater 迁移）仍待执行**：按推荐做法，需要先发一版过渡的 Electron 版把更新提示改成
「请手动下载」，并保持 NSIS 的 appId/productName 不变让 Tauri 安装器能原地覆盖安装。
这件事要在真正启用 `tauri-plugin-updater` 之前做掉。
