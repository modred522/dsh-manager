# DSH Manager

[![check](https://github.com/modred522/dsh-manager/actions/workflows/check.yml/badge.svg)](https://github.com/modred522/dsh-manager/actions/workflows/check.yml)
[![release](https://img.shields.io/github/v/release/modred522/dsh-manager)](https://github.com/modred522/dsh-manager/releases)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![tauri](https://img.shields.io/badge/Tauri-2-24C8D8.svg)](https://tauri.app/)

A desktop manager for DeepSeek Harness (dsh): tray-resident, one-click start/stop, update management, plugin marketplace with AI analysis, and token usage statistics.

The backend is **Rust + Tauri 2** (the original Electron build has been retired; see `docs/TAURI-MIGRATION.md`): a 2.3 MB installer and about 40 MB resident.

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

- **Run**: double-click the "DSH 管理器" desktop shortcut, or launch `DSH Manager.exe` from the install directory.
- **Quit**: right-click the tray icon → 退出 (closing the window only minimizes to tray).
- **Global shortcut**: `Ctrl+Alt+D` opens DSH quickly.

## Installation (from source)

```powershell
npm install -g @deepseek-ai/dsh      # prerequisite: the DeepSeek Harness CLI
# Also needed: Rust stable (https://rustup.rs) and the WebView2 Runtime (bundled with Win11)
git clone https://github.com/modred522/dsh-manager.git && cd dsh-manager
npm install                          # installs only @tauri-apps/cli
npx tauri build                      # builds the installer; use npx tauri dev to just run it
```

## Installation (Release)

Node.js and the dsh CLI are still required on the machine (the manager uses them to start/update DSH).

1. Download the latest `DSH Manager_<version>_x64-setup.exe` (about 2.3 MB) from
   [Releases](https://github.com/modred522/dsh-manager/releases)
2. Run the installer (current user, no admin needed); desktop and start-menu shortcuts
   are created for you
3. From then on the manager **checks** for a newer version at startup and points you at
   the releases page — the "检查管理器新版本" switch under Overview → Settings controls it

> **It does not download and install updates by itself.** Tauri's updater mandates an
> Ed25519 signature, which means holding a private key indefinitely: lose it and no
> installed client can ever auto-update again. On Windows it downloads an installer and
> runs it anyway, so the installer UI shows up regardless. Reasoning in
> `docs/TAURI-MIGRATION.md` section 16.

Releasing (maintainers): **push a tag to release automatically** — GitHub Actions builds and creates the Release on GitHub's servers:

```powershell
git tag v1.0.2
git push origin v1.0.2
```

Local manual build (fallback):

```powershell
npx tauri build   # artifacts land in src-tauri\target\release\bundle\nsis\
```

The release workflow runs `tools/set-version.js` to stamp the tag's version into
`package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`. Miss the
Cargo.toml one and shipped clients believe they are the repo's placeholder version
forever.

## Project Layout

```
dsh-manager
├── src-tauri/              # Rust backend
│   ├── src/                # lib.rs (commands + shell) plus 13 modules; see ARCHITECTURE.md
│   ├── capabilities/       # Tauri 2 permissions (you must write these; without them the
│   │                       # frontend receives no events at all)
│   ├── tauri.conf.json     # app metadata and NSIS bundle config
│   └── Cargo.toml
├── renderer/               # frontend (plain JS: no framework, no build step)
│   ├── index.html + renderer.js + styles.css + i18n.js
│   ├── market.html + market.js      # marketplace window
│   └── tauri-bridge.js              # maps window.dsh.* onto invoke/listen
├── .github/workflows/      # check (cargo + renderer self-check) / release (push a v* tag)
├── tools/
│   ├── check-i18n.js       # renderer self-check: i18n keys / DOM ids / shadowed globals / CSS
│   ├── set-version.js      # stamps the version into all three manifests at release time
│   ├── render-icon.ps1     # whale icon generator (run in STA)
│   ├── social-preview.ps1  # GitHub social preview card generator (1280x640)
│   └── publish.ps1         # one-click publish to GitHub
├── assets/                 # whale.png / app.ico / social-preview.png / screenshots/
└── docs/                   # MEMORY.md (handover) / ARCHITECTURE.md / GOTCHAS.md
                            # TAURI-MIGRATION.md (the whole Electron to Tauri move)
```

## Development

```powershell
npm install                     # installs only @tauri-apps/cli
npx tauri dev                   # development mode
npx tauri build                 # produces the NSIS installer
node tools\check-i18n.js        # renderer self-check

cd src-tauri
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test

powershell -NoProfile -STA -File tools\render-icon.ps1  # regenerate the whale icon
powershell -NoProfile -File tools\publish.ps1  # one-click publish to your own GitHub repo (after forking)
```

> **Do not point a desktop shortcut at a `tauri dev` build.** The debug binary is a
> console-subsystem executable (so it flashes a black window) and its frontend points at
> the dev server, so launching it outside `tauri dev` only shows "can't reach this page".
> The app itself now refuses to write a shortcut from a debug build.

## Docs

- **`docs/MEMORY.md`**: project handover memory (read first)
- **`docs/ARCHITECTURE.md`**: code map, IPC contract, data structures, extension guide
- **`docs/GOTCHAS.md`**: pitfalls and the delivery checklist

## Requirements

- Runtime: Windows 10/11 with the WebView2 Runtime (bundled with Win11)
- Development: Rust stable (1.80+), Node.js 24 / npm (only to install `@tauri-apps/cli`)
- dsh (npm global, `@deepseek-ai/dsh`), DSH_HOME defaults to `~\.dsh`

## Credits

- The whale icon is rendered from the DeepSeek Harness (dsh, MIT License) favicon path.

## License

[MIT](LICENSE)
