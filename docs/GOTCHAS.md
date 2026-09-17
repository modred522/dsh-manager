# DSH 管理器 — 踩坑记录（血泪教训，改代码前先读）

## 一、PowerShell 编码与语法（最高频坑）

1. **Windows PowerShell 5.1 读无 BOM 的 .ps1 按 ANSI/GBK 解析**。UTF-8 中文注释的字节会吞掉换行符 → 注释行与下一行合并 → 变量未赋值/语法错误。
   - 症状：`Unexpected token '}'`；或变量为空报 `Empty path name is not legal`（icoPath 未赋值）。
   - **规则：项目里所有 .ps1 必须纯 ASCII（英文注释）**。find-dsh.ps1 和 tools/render-icon.ps1 均已按此重写，勿再加中文注释。
2. **`$PID` 是 PowerShell 只读自动变量**（大小写不敏感）。用 `$pid` 当变量名会报 `Cannot overwrite variable PID`，且若在 `catch {}` 里会被静默吞掉。用 `$procId` 等其它名字。
3. 外部命令输出捕获：`netstat -ano 2>$null` 可用；空 `catch {}` 块尽量不用（错误被吞难以排查）。
4. 图标渲染脚本需要 **STA**（WPF 渲染）：用 `powershell.exe -NoProfile -STA -File ...` 运行。
5. **处理 UTF-8 无 BOM 文件（package.json 等）**：PS 5.1 的 `Get-Content` 按 ANSI 读 → 中文变乱码、JSON 解析失败。用 `[System.IO.File]::ReadAllText()` 读。
6. **写回 JSON 别用 `Set-Content -Encoding UTF8`**（PS 5.1 会加 BOM），electron-rebuild/JSON.parse 不认 BOM。用 `[System.IO.File]::WriteAllText()`（默认无 BOM）。
7. **ConvertFrom-Json 的 PSCustomObject 不能 `$obj.新属性 = x`**（属性不存在时抛错），用 `Add-Member -NotePropertyName ... -NotePropertyValue ...`。
8. **New-Object 构造参数里别写带运算的逗号表达式**：`Point($x + 18, 448)` 被解析成数组相加报 `op_Addition`，先算变量再传参。

## 二、开发环境（DSH 会话沙箱）限制

> 这些只影响**我在沙箱里的验证**，用户真实环境不受影响。交付前想清楚哪些能测哪些不能。

1. **Node `spawn`/`exec` 带管道 stdio 会同步抛 EPERM**（capture 子进程输出被沙箱禁止）。→ 所有 spawn 调用必须 try/catch 同步异常（main.js 已全部包好）。
2. **WMI 被禁**：`Get-CimInstance Win32_Process` 返回"拒绝访问"；wmic 同样。→ 沙箱里无法验证进程命令行/CPU 采样。
3. **Get-NetTCPConnection 返回空**，但 `netstat -ano` 可用、`Get-Process` 可用、Node `fetch` 网络可用（PowerShell Invoke-RestMethod 的 SSL 会失败）。
4. **GUI 在沙箱里跑不起来**：Electron 时代是 Chromium 沙箱对 userData 目录报 `拒绝访问 / network_sandbox` FATAL；换成 Tauri 后同样起不来（WebView2 需要真实桌面会话）。**都不是代码问题**。验证方式：`cargo test` 逻辑单测 + `npx tauri build` + **隔离启动烟测**（验产物本身能起、不崩、不乱动环境）+ 让用户真机验收。
5. 跑 dsh 相关命令需注意：`npm view/install` 会写 npm 缓存（沙箱 EPERM）；可设 `$env:npm_config_cache` 到工作区内目录绕过。

## 三、历史：Electron 时代的打包坑（后端已退场，教训仍然成立）

Electron 后端已经删除，这一节只保留有普遍价值的那条。

**`electron-builder.yml` 的 `files` 是白名单，漏列的源码目录会被静默丢掉**（当时
`asar: false`，只拷白名单）。v1.0.5 就是这么发坏的：`lib/` 拆分后没加进 `files`，
而 `main.js` 顶层 `require('./lib/pure')` → 装出来的应用一启动就
`Error: Cannot find module './lib/pure'`，**从发布到发现坏了 3 周多**。更容易上当的是
不对称：`node_modules` 的生产依赖被特殊处理、自动打进去（所以 `electron-updater` 在、
`lib/` 不在）。

**教训（这才是要带走的）：`node --check` 过了不代表装得起来 —— 只有对产物本身做核对
才拦得住。** 现在这条防线是隔离启动烟测：跑真实的 `tauri build` 产物，验它能起、
不崩、不乱动用户环境、前端没有未捕获异常。Tauri 侧没有 `files` 白名单这种东西
（前端资源直接嵌进 exe），但"只信产物、不信源码检查"的原则照旧。


## 四、dsh 集成坑

1. **headless profile 默认没有模型适配器**：默认模型走 `deepseek-modlens`（settings.yaml），headless 没装 modlens → 报 `NO_ADAPTER: no adapter registered for provider "modlens-deepseek"`。解决方案（已内置）：`ensureAnalysisEnv` 把 web profile 的非核心 bundles 自动补装到 headless（`dsh plugin --profile headless add <pkg>`，实测 2.2 秒装好）。
2. **`dsh plugin add` 会自动把插件写进 profile 的 `dsh.profile.bundles`**（不只 dependencies）——不要手改。
3. **GitHub 插件安装被 pnpm 安全闸门拦**：git 依赖的 `prepare` 构建脚本默认禁止 → 报错提示把包名加入 `profiles\<p>\pnpm-workspace.yaml` 的 `allowBuilds`。管理器流程：渲染层弹供应链风险确认 → 写 allowBuilds → 重试安装。
4. dsh 进程命令行 = `node "...\@deepseek-ai\dsh\lib\bin.js" <profile>`；npm shim 用 `%dp0%\` 拼接导致命令行里有**双反斜杠**（正常现象，显示前归一化即可）。
5. 检测 dsh 进程**别只靠命令行匹配**（历史上失败过）：netstat 端口（3080）是可靠主路径。
6. 更新 dsh = `npm install -g @deepseek-ai/dsh@<ver>`；无内置回滚 → 管理器自己记录旧版本号实现回滚。
7. **会话持久化根在 `dsh-base` 的 `cordis.patch.yml` 里**（`session-persistence-jsonl` 行，`root: !!js dshHomePath('sessions')`）。管理器的会话隔离靠 `dsh --patch <file>` 覆盖该行——注意两点：① 补丁是**整行 config 替换**（只写 root 即可，其余键有默认值）；② dsh 升级若改行 id 会静默失效（匹配不到只 warn），管理器启动时 `verifyHeadlessPatchRow()` 检查并告警。持久层**无删除接口**，删文件是正规途径，但只删管理器自己的 `analysis-sessions` 目录，**绝不碰 `$DSH_HOME/sessions` 下用户会话**。
8. **检查 dsh 更新不能只读 `npm view <pkg> version`（= latest 标签）**：dsh 的 rc 预发行版惯例挂在 `next` 标签上（实测 rc.8 在 next、rc.7 在 latest），只读 latest 会漏报。正确做法：`npm view dist-tags --json` 取所有标签的最高版本。且版本比较要用 semver 规则逐段比预发布（rc.10 > rc.9，纯字符串比较会错），main.js 与 renderer/renderer.js 各有一份 compareVersions，**两处都要同步改**。

## 五、UI / CSS 坑

1. **`hidden` 属性会被 CSS `display:flex` 覆盖** → 弹窗/面板"永远显示"。必须保留全局规则 `[hidden]{display:none!important}`。
2. **窗口高度不足时日志卡被挤压裁切**：`.tab-page` 要 `overflow-y:auto`；常规卡片 `flex-shrink:0`；`.log-card` `flex:1 1 auto; min-height:140px`。
3. CSP：`style-src` 加了 `'unsafe-inline'`（图表用 CSSOM/内联宽度）；`img-src 'self' data:`；JS 一律用 `textContent`/DOM 构建，防注入。
4. 深色模式：颜色全部走 CSS 变量（`:root` 与 `body.dark` 两套）；鲸鱼 Logo 深色下 `filter:invert(1)`。

## 六、交付检查清单（每次改完跑一遍）

- [ ] `cd src-tauri && cargo fmt --all --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test`
- [ ] `node tools/check-i18n.js` —— i18n 键集对齐 + `$('id')` 与 HTML 的 `id=` 交叉核对
      + **共享全局不被遮蔽**（`renderUsage` 里一个 `const t` 就让用量页少渲染两块）
      + **CSS 写死宽度 + nowrap 的溢出风险**（用量页曾因此多出一条横向滚动条）
- [ ] `node --check` 渲染层四个 JS（renderer / market / i18n / tauri-bridge）
- [ ] `npx tauri build` + **隔离启动烟测**（`DSH_MANAGER_DATA` 重定向数据目录、
      配置里关掉桌面快捷方式与自启、用 `tasklist` 判存活、断言日志里没有 `[前端错误]`）
- [ ] 新增命令：`generate_handler!` 里注册了吗？建窗命令写成 `async` 了吗？
- [ ] 新增事件：`capabilities/default.json` 覆盖到了吗？改完 **`touch src-tauri/build.rs`**
      强制 build script 重跑，否则产物里还是旧 ACL（这个坑让我一度以为修复没生效）
- [ ] 涉及 .ps1：确认纯 ASCII + 在 `powershell.exe`（5.1）下跑通
- [ ] 涉及凭据 / 令牌：确认完整值既不回传渲染层、也不进日志（见 `token.rs` 的三条铁律）
- [ ] 涉及发版：`node tools/set-version.js <版本>` 必须把三处 manifest 都写到
      （漏了 `Cargo.toml` 的话，装出去的客户端永远认为自己是占位版本）
- [ ] 提交前：`git log --format='%an <%ae>'` 确认作者是仓库既有身份，不是机器的全局配置
- [ ] 提交前：锁文件只指向 `registry.npmjs.org`（本机 npm 默认走内网镜像，
      任何一次 `npm install` 都会把内网主机名写回去）
- [ ] 告知用户：重启管理器后验证（沙箱里无法 GUI 实测）
