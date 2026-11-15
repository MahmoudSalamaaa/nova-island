# What the Electron (NSIS) install leaves on a PC (recorded 2026-11-03, developer PC)

The 1.8 installer and the move from 1.7 are built against this.

## Uninstall entry
`HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\a9c08701-fc32-5535-8c18-9dda7436a24c`
(the GUID is electron-builder's UUID v5 of the appId `com.dynamicnova.app`; 1.8 must reuse it so
"Installed apps" keeps showing one Nova Island)

| Value | Data on this PC |
|---|---|
| DisplayName | `Nova Island 1.3.0` (stale: updates since 1.3.0 didn't refresh it) |
| DisplayVersion | `1.3.0` (stale too) |
| UninstallString | `"…\Programs\Dynamic Nova\Uninstall Nova Island.exe" /currentuser` |
| QuietUninstallString | same + ` /S` |
| DisplayIcon | `…\Programs\Dynamic Nova\uninstallerIcon.ico` |
| Publisher | `AmerTech` |
| Comments | `A living island for your Windows desktop` |
| NoModify / NoRepair | 1 / 1 |

## Install folder
- The app runs from `%LOCALAPPDATA%\Programs\Dynamic Nova\Nova Island\Nova Island.exe`
  (Start-menu shortcut target, working dir = that folder).
- The parent `…\Programs\Dynamic Nova\` (old product name) holds the uninstaller and only the
  `Nova Island` subfolder. 1.8 must read the real folder from the shortcut or the running exe, not
  assume `Programs\Nova Island`.

## Start with Windows
- `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value `electron.app.Nova Island`
  = `"%LOCALAPPDATA%\Programs\Dynamic Nova\Nova Island.exe"`
- `…\Explorer\StartupApproved\Run`, `electron.app.Nova Island` = `02 00 …` (enabled)
- **Bug found in 1.7:** that Run path no longer exists (the app moved into the `Nova Island`
  subfolder), so start-with-Windows silently does nothing on this PC. `setLoginItemSettings` stores
  the exe path only when the option is toggled. 1.8 must rewrite the Run value with the current exe
  path at every start while the option is on.

## Shortcuts
- Start menu: `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Nova Island.lnk` → the exe above.
- Desktop: none on this PC. Startup folder: none (the old dev shortcut was migrated away).

## Updater
`%LOCALAPPDATA%\nova-island-updater\`: `installer.exe` (last installer run), `current.blockmap`,
`pending\` with a leftover `Nova-Island-Setup-1.4.0.exe` (112 MB) and `update-info.json`
(`fileName`, `sha512`, `isAdminRightsRequired`). Stale downloads are never cleaned; 1.8 should clear
this folder once after migrating (~230 MB on this PC).

## Data
`%APPDATA%\dynamic-nova\`: the app's JSON files (settings, features, apps, apps-cache, shelf, mini,
location-cache, prayer-cache, weather-cache, news-seen, plugins, plugins-cache), `plugins\`,
`nova.log`, the `renamed` marker, plus Chromium's own folders (Cache, GPUCache, Local Storage,
Session Storage, …). 1.8 keeps the JSON files and `plugins\`, imports `Local Storage` once, and can
delete the Chromium folders after migrating.
