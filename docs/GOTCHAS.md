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
4. **Electron GUI 在沙箱里跑不起来**：Chromium 原生沙箱对 userData 目录报 `拒绝访问 / network_sandbox` FATAL——**不是代码问题**。验证方式：`node --check` + 逻辑单测 + 让用户真机验收。
5. 跑 dsh 相关命令需注意：`npm view/install` 会写 npm 缓存（沙箱 EPERM）；可设 `$env:npm_config_cache` 到工作区内目录绕过。

## 三、Electron 安装与运行

1. **安装 electron 的二进制下载缓存环境变量是 `electron_config_cache`**（@electron/get 读取），**不是** `ELECTRON_CACHE`。设置错会报 `EPERM: mkdir 'C:\Users\...\AppData\Local\electron'`。安装流程：设该变量 → `npm install` → 若 `node_modules\electron\dist\electron.exe` 缺失则手动 `node node_modules\electron\install.js`。
2. **单实例锁的坑**：改了代码交付时，用户双击快捷方式只会唤醒**旧实例**（旧代码）→ "没有新功能"。交付前必须：`Get-Process electron | Stop-Process -Force`（taskkill 有时报 Access denied，Stop-Process 更稳；个别残留 crashpad_handler 可能杀不掉，占着旧目录句柄）。
3. 未打包应用的路径约定：`process.execPath` = electron.exe；`app.getAppPath()` = 项目目录；桌面快捷方式 = `TargetPath=electron.exe` + `Arguments="D:\DSHManager"` + `Icon=assets\app.ico`。
4. `shell.writeShortcutLink` 操作用 `'replace'`（不是 'create'，否则文件已存在会失败）。

## 四、dsh 集成坑

1. **headless profile 默认没有模型适配器**：默认模型走 `deepseek-modlens`（settings.yaml），headless 没装 modlens → 报 `NO_ADAPTER: no adapter registered for provider "modlens-deepseek"`。解决方案（已内置）：`ensureAnalysisEnv` 把 web profile 的非核心 bundles 自动补装到 headless（`dsh plugin --profile headless add <pkg>`，实测 2.2 秒装好）。
2. **`dsh plugin add` 会自动把插件写进 profile 的 `dsh.profile.bundles`**（不只 dependencies）——不要手改。
3. **GitHub 插件安装被 pnpm 安全闸门拦**：git 依赖的 `prepare` 构建脚本默认禁止 → 报错提示把包名加入 `profiles\<p>\pnpm-workspace.yaml` 的 `allowBuilds`。管理器流程：渲染层弹供应链风险确认 → 写 allowBuilds → 重试安装。
4. dsh 进程命令行 = `node "...\@deepseek-ai\dsh\lib\bin.js" <profile>`；npm shim 用 `%dp0%\` 拼接导致命令行里有**双反斜杠**（正常现象，显示前归一化即可）。
5. 检测 dsh 进程**别只靠命令行匹配**（历史上失败过）：netstat 端口（3080）是可靠主路径。
6. 更新 dsh = `npm install -g @deepseek-ai/dsh@<ver>`；无内置回滚 → 管理器自己记录旧版本号实现回滚。
7. **会话持久化根在 `dsh-base` 的 `cordis.patch.yml` 里**（`session-persistence-jsonl` 行，`root: !!js dshHomePath('sessions')`）。管理器的会话隔离靠 `dsh --patch <file>` 覆盖该行——注意两点：① 补丁是**整行 config 替换**（只写 root 即可，其余键有默认值）；② dsh 升级若改行 id 会静默失效（匹配不到只 warn），管理器启动时 `verifyHeadlessPatchRow()` 检查并告警。持久层**无删除接口**，删文件是正规途径，但只删管理器自己的 `analysis-sessions` 目录，**绝不碰 `$DSH_HOME/sessions` 下用户会话**。

## 五、UI / CSS 坑

1. **`hidden` 属性会被 CSS `display:flex` 覆盖** → 弹窗/面板"永远显示"。必须保留全局规则 `[hidden]{display:none!important}`。
2. **窗口高度不足时日志卡被挤压裁切**：`.tab-page` 要 `overflow-y:auto`；常规卡片 `flex-shrink:0`；`.log-card` `flex:1 1 auto; min-height:140px`。
3. CSP：`style-src` 加了 `'unsafe-inline'`（图表用 CSSOM/内联宽度）；`img-src 'self' data:`；JS 一律用 `textContent`/DOM 构建，防注入。
4. 深色模式：颜色全部走 CSS 变量（`:root` 与 `body.dark` 两套）；鲸鱼 Logo 深色下 `filter:invert(1)`。

## 六、交付检查清单（每次改完跑一遍）

- [ ] `node --check` 四个 JS 文件（main / preload / renderer / market）
- [ ] renderer.js 的 `$('id')` 与 index.html 的 `id=` 交叉核对（历史上靠这个抓过缺元素）
- [ ] preload 的 invoke 通道与 main.js 的 ipcMain.handle 一一对应
- [ ] 涉及 .ps1：确认纯 ASCII + 在 `powershell.exe`（5.1）下跑通
- [ ] `Get-Process electron | Stop-Process -Force` 杀旧实例
- [ ] 告知用户：重启管理器后验证（沙箱里无法 GUI 实测）
- [ ] 发布公共仓库前：确认无个人路径/密钥，node_modules 被 .gitignore 排除
