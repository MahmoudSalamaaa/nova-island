# nova-next: the Nova Island app

This folder is the app that ships: Nova Island 1.8 and later, written in Rust. It replaced the
Electron app (1.7 and earlier, kept in git history and the `electron-baseline-1.7.0` tag).

The island, Settings and the mini player are web pages shown in WebView2 (part of Windows 10/11).
Those pages live at the repository root (`index.html`, `renderer.js`, `views/`, `prefs/`, `mini/`,
`style.css`, `themes.css`); everything else, from the window to the system integration, is Rust.

## Layout
| Folder | What |
|---|---|
| `crates/nova-app` | The app: the island window, its behaviour (`core.rs`), the page channels (`router.rs`), Settings, widgets, the taskbar strip, the adhan |
| `crates/nova-services` | Prayer times, weather, network, apps, downloads, shelf, calendar, news, plugins, updates |
| `crates/nova-sys` | Windows APIs: media sessions, volume and brightness, clipboard, apps, taskbar, UI Automation |
| `crates/nova-web` | The WebView2 page: serving the embedded files, the bridge to the app |
| `crates/nova-render` | Native drawing with Direct2D: the desktop widgets and the taskbar strip |
| `crates/nova-assets` | Embeds the pages in the exe at build time (see below) |
| `crates/nova-core`, `nova-prayer`, `nova-sign` | Settings and features, prayer-time maths, plugin and news signatures |
| `pages/` | Pages and files added to (or replacing) the root ones |
| `plugins/` | The built-in plugins (signed `.nvp`) |
| `installer/` | The GitHub installer (NSIS) and the Microsoft Store package (MSIX) |
| `parity/` | Reference outputs from the old JavaScript, used by the tests |
| `contract/` | Every channel between the pages and the app |
| `tools/` | Test helpers, and the scripts that build the plugins |

## How the pages are put together
`crates/nova-assets/build.rs` embeds the pages into the exe:
- The root files, as they are.
- Files in `nova-next/pages/` are added, or replace a root file with the same path.
- `PATCHES` makes small text edits on root files. Each one must match exactly once, so a change at
  the root that breaks a patch stops the build and says which one.

The pages talk to the app through `window.nova` (island), `window.prefs` (Settings) and
`window.mini`, defined by the `preload.js` files. These run inside WebView2 on a small
`require('electron')` shim, and `crates/nova-app/src/router.rs` answers every channel.

## Build
Requirements: Windows 10/11 (64-bit), [Rust](https://rustup.rs) with the MSVC toolchain, and the
WebView2 runtime (already part of Windows 11).

```bash
cd nova-next
cargo build --release -p nova-app     # target/release/nova-island.exe
cargo test --workspace
```

A copy run from Cargo's target folder keeps its settings in `<target>/data`, not your installed
Nova's. Useful flags: `--data <folder>`, `--open-settings --settings-page <id>`,
`--devtools-port 9333` (then `node tools/test/cdp.mjs` drives the pages).

The installers (optional) also need Python with Pillow, [NSIS 3](https://nsis.sourceforge.io)
for the GitHub installer, and the Windows SDK (`makeappx`) for the Store package:

```bash
powershell -ExecutionPolicy Bypass -File installer\build.ps1        # <target>\dist\Nova-Island-Setup-<version>.exe
powershell -ExecutionPolicy Bypass -File installer\msix\build.ps1   # <target>\dist\Nova-Island-<version>-Store.msixupload
```

The adhan recordings (`pages/sounds/*.mp3`) aren't in the repository. Without them the build works,
and Settings › Prayer & Dhikr asks for an adhan file.
