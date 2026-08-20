# DSH Manager

[![check](https://github.com/modred522/dsh-manager/actions/workflows/check.yml/badge.svg)](https://github.com/modred522/dsh-manager/actions/workflows/check.yml)
[![release](https://img.shields.io/github/v/release/modred522/dsh-manager)](https://github.com/modred522/dsh-manager/releases)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![electron](https://img.shields.io/badge/Electron-43.4.0-47848F.svg)](https://www.electronjs.org/)

An Electron desktop manager for DeepSeek Harness (dsh): tray-resident, one-click start/stop, update management, plugin marketplace with AI analysis, and token usage statistics.

[中文](README.md) | English

## Screenshots

<table>
  <tr>
    <td align="center"><b>Overview</b> (version / processes / updates / settings / logs)<br><img src="assets/screenshots/overview.png" width="430" alt="Overview"></td>
    <td align="center"><b>Usage</b> (token stats / per-project ranking / 14-day trend)<br><img src="assets/screenshots/usage.png" width="430" alt="Usage"></td>
  </tr>
  <tr>
    <td align="center"><b>Plugins</b> (installed plugins / quick install)<br><img src="assets/screenshots/plugins.png" width="430" alt="Plugins"></td>
    <td align="center"><b>Plugin Marketplace</b> (standalone window, npm / GitHub sources)<br><img src="assets/screenshots/market.png" width="430" alt="Marketplace"></td>
  </tr>
  <tr>
    <td align="center" colspan="2"><b>Plugin detail + one-click analysis</b> (README / score card / console, resizable panes)<br><img src="assets/screenshots/analysis.png" width="720" alt="Plugin detail"></td>
  </tr>
</table>

## Features

| Tab | Description |
|---|---|
| Overview | Start/restart/stop DSH (detects **all** dsh processes), process panel (PID / memory / CPU / stop individually), check for updates / update now / rollback (with official changelog), settings (auto-start / watchdog / silent startup / theme / URL / interval), logs (7-day persistence + export), tools (config dir / install dir / about) |
| Usage | Token statistics: totals, per-project ranking, 14-day trend, cost estimation (adjustable unit prices) |
| Plugins | Installed plugin management + quick npm install; the **plugin marketplace** (npm + GitHub search) runs in its own **standalone window**; every card shows a direct GitHub page link (opens in your system browser); the plugin detail page is a full-window view (README / score card / console with **draggable split panes**), plus **one-click analysis** (dsh headless evaluates whether a plugin is "genuinely useful or just hype", structured score card + history cache), install/uninstall (GitHub source with supply-chain risk confirmation) |

## Quick Start

- **Run**: double-click the "DSH 管理器" desktop shortcut, or run `npm start` in the project directory.
- **Quit**: right-click the tray icon → 退出 (closing the window only minimizes to tray).
- **Global shortcut**: `Ctrl+Alt+D` opens DSH quickly.

## Installation (from source)

```powershell
npm install -g @deepseek-ai/dsh      # prerequisite: the DeepSeek Harness CLI
git clone https://github.com/modred522/dsh-manager.git && cd dsh-manager
npm install
npm start
```

## Installation (Release)

Portable zip: extract and run, no Electron dev environment needed. Node.js and the dsh CLI are still required on the machine (the manager uses them to start/update DSH).

1. Download the latest `DSH-Manager-<version>-win.zip` from [Releases](https://github.com/modred522/dsh-manager/releases)
2. Extract it anywhere and double-click `DSH Manager.exe`
3. Optionally create a desktop shortcut afterwards via "Overview → Tools"; the app lives in the system tray

Building a release (maintainers): packaging happens in a **separate directory**, keeping the source tree clean:

```powershell
powershell -NoProfile -File tools\build-release.ps1   # output to D:\DSHManagerRelease\release
```

## Project Layout

```
dsh-manager
├── main.js                 # all main-process logic
├── preload.js              # IPC bridge (window.dsh)
├── find-dsh.ps1            # WMI details for dsh processes (pure ASCII)
├── .github/workflows/      # CI (syntax checks on push/PR)
├── tools/
│   ├── render-icon.ps1     # whale icon generator (run in STA)
│   ├── social-preview.ps1  # GitHub social preview card generator (1280x640)
│   ├── build-release.ps1   # package the release zip in a separate directory
│   └── publish.ps1         # one-click publish to GitHub
├── assets/                 # whale.png / app.ico / social-preview.png / screenshots/
├── renderer/               # main window: index.html + styles.css + renderer.js
│                           # marketplace window: market.html + market.js
└── docs/                   # MEMORY.md (handover) / ARCHITECTURE.md / GOTCHAS.md
```

## Development

```powershell
npm start                                  # launch the app
node --check main.js                       # syntax check (preload.js, renderer/*.js too)
Get-Process electron | Stop-Process -Force # kill old instances after code changes
powershell -NoProfile -STA -File tools\render-icon.ps1  # regenerate the whale icon
powershell -NoProfile -File tools\publish.ps1  # one-click publish to your own GitHub repo (after forking)
```

## Docs

- **`docs/MEMORY.md`**: project handover memory (read first)
- **`docs/ARCHITECTURE.md`**: code map, IPC contract, data structures, extension guide
- **`docs/GOTCHAS.md`**: pitfalls and the delivery checklist

## Requirements

- Node.js 24 / npm 11 / Electron 43.4.0 (installed via `npm install`)
- dsh (npm global, `@deepseek-ai/dsh`), DSH_HOME defaults to `~\.dsh`

## Credits

- The whale icon is rendered from the DeepSeek Harness (dsh, MIT License) favicon path.

## License

[MIT](LICENSE)
