# Shipped Store package (1.7.0), for reference

`AppxManifest-1.7.0.xml` is copied from `dist/Nova-Island-1.7.0-Store.appx` as it went to the Store.
The 1.8 manifest must keep these exactly, or the Store update breaks something for existing users:

| Field | 1.7.0 value | Why it must not change |
|---|---|---|
| `Identity/@Name` | `AmerTech.NovaIsland` | Same Store product |
| `Identity/@Publisher` | `CN=8BA7D487-B6B1-48CC-A9BE-EAC4A654C421` | Same Store product |
| `Application/@Id` | `NovaIsland` | Start menu and taskbar pins |
| `Application/@Executable` | `app\Nova Island.exe` | Keep the `app\` folder in the 1.8 layout |
| `StartupTask/@TaskId` | `SlackStartup` | The user's "start with Windows" choice is stored under this id (electron-builder's default name; keep it) |
| `TargetDeviceFamily/@MinVersion` | `10.0.17763.0` | Windows 10 1809, also SMTC's minimum |

The Rust app ships as version `<version>.0` (1.8.0.0 and later).
