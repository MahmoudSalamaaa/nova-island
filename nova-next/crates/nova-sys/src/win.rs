//! The window in front, full screen, keyboard layout and Caps Lock, the clipboard's change counter
//! and private copies, idle time, battery, screen recorders, and the browser in front (Win in
//! NovaHelper.cs). Rectangles are physical pixels; nova-app converts them to DIPs.
use windows::{
    core::{w, PWSTR},
    Win32::{
        Foundation::{CloseHandle, HANDLE, HWND, RECT},
        Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST},
        System::{
            DataExchange::{CloseClipboard, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW},
            Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS},
            Memory::{GlobalLock, GlobalUnlock},
            Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS},
            SystemInformation::GetTickCount,
            Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION},
        },
        UI::{
            Input::KeyboardAndMouse::{GetKeyState, GetKeyboardLayout, GetLastInputInfo, LASTINPUTINFO},
            WindowsAndMessaging::{GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId},
        },
    },
};

pub fn foreground() -> HWND {
    unsafe { GetForegroundWindow() }
}

pub fn class_of(h: HWND) -> String {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// The exe name without ".exe", like .NET's Process.ProcessName ("" when it can't be read).
pub fn process_name(pid: u32) -> String {
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return String::new() };
        let mut buf = [0u16; 1024];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
        let _ = CloseHandle(h);
        if !ok {
            return String::new();
        }
        let path = String::from_utf16_lossy(&buf[..n as usize]);
        let file = path.rsplit('\\').next().unwrap_or("");
        match file.rfind('.') {
            Some(i) if file[i..].eq_ignore_ascii_case(".exe") => file[..i].to_string(),
            _ => file.to_string(),
        }
    }
}

/// (process name, window title) of the window in front.
pub fn foreground_info() -> (String, String) {
    let h = foreground();
    if h.is_invalid() {
        return (String::new(), String::new());
    }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(h, &mut buf) };
    (process_name(pid), String::from_utf16_lossy(&buf[..n.max(0) as usize]))
}

/// The window in front covers its whole monitor (the desktop and taskbar don't count).
pub fn is_fullscreen() -> bool {
    let h = foreground();
    if h.is_invalid() {
        return false;
    }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
    if !may_hide_island(&class_of(h), &process_name(pid)) {
        return false;
    }
    unsafe {
        let mut r = RECT::default();
        if GetWindowRect(h, &mut r).is_err() {
            return false;
        }
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST), &mut mi).as_bool() {
            return false;
        }
        let m = mi.rcMonitor;
        r.left == m.left && r.top == m.top && r.right == m.right && r.bottom == m.bottom
    }
}

/// Whether a window covering the whole screen counts as "full screen" (Hide in full screen). Not
/// for what only looks full screen but is the user at work, each a bug that came back once:
///   the desktop and the taskbar
///   File Explorer in full screen (F11): it hid the island, and every photo opened from it came
///     back to a hidden island (1.8.1)
///   the Snipping Tool's overlay (Win+Shift+S): the island hid, so no screenshot had it (1.8.3)
pub fn may_hide_island(class: &str, process: &str) -> bool {
    if matches!(class, "Progman" | "WorkerW" | "Shell_TrayWnd" | "CabinetWClass" | "ExploreWClass") {
        return false;
    }
    let p = process.to_ascii_lowercase();
    !matches!(p.as_str(), "snippingtool" | "screenclippinghost" | "screensketch")
}

/// The keyboard layout (LANGID) of the window in front's thread.
pub fn keyboard_layout() -> i64 {
    unsafe {
        let h = foreground();
        let thread = if h.is_invalid() { 0 } else { GetWindowThreadProcessId(h, None) };
        (GetKeyboardLayout(thread).0 as usize & 0xffff) as i64
    }
}

pub fn caps_lock() -> bool {
    unsafe { (GetKeyState(0x14) & 1) != 0 }
}

pub fn clipboard_seq() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

/// Password managers mark what they copy so history tools skip it: either the "Exclude..." format
/// is present, or CanIncludeInClipboardHistory is 0. Checked once per clipboard change.
pub struct Private {
    seq: Option<u32>,
    value: bool,
    // the last change whose answer is known, as reported; and tries on the one still open
    shown: Option<(u32, bool)>,
    tries: u32,
}

/// Ticks (500 ms each) to wait for a clipboard someone still holds, before calling it private.
const PRIVATE_TRIES: u32 = 4;

impl Private {
    pub fn new() -> Private {
        Private { seq: None, value: false, shown: None, tries: 0 }
    }

    /// Private or not, for the clipboard as it is (`seq`). None: it can't be read right now.
    pub fn check(&mut self, seq: u32) -> Option<bool> {
        if self.seq == Some(seq) {
            return Some(self.value);
        }
        let value = unsafe { Self::look() }?;
        self.seq = Some(seq);
        self.value = value;
        Some(value)
    }

    /// (sequence number, private) to report: a new change only once its answer is known, or after
    /// PRIVATE_TRIES ticks of a clipboard nobody can open (then private, as before). Before, a
    /// clipboard still held by the app that copied (the Snipping Tool, writing a screenshot) was
    /// called private at once, so the screenshot toast and its thumbnail never came.
    pub fn settled(&mut self, seq: u32) -> (u32, bool) {
        match self.check(seq) {
            Some(p) => {
                self.tries = 0;
                self.shown = Some((seq, p));
            }
            None => {
                self.tries += 1;
                if self.tries >= PRIVATE_TRIES || self.shown.is_none() {
                    self.tries = 0;
                    self.seq = Some(seq);
                    self.value = true;
                    self.shown = Some((seq, true));
                }
            }
        }
        self.shown.unwrap_or((seq, true))
    }

    /// None: the clipboard is held by someone else just now.
    unsafe fn look() -> Option<bool> {
        if IsClipboardFormatAvailable(RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"))).is_ok() {
            return Some(true);
        }
        let can = RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory"));
        if IsClipboardFormatAvailable(can).is_err() {
            return Some(false);
        }
        OpenClipboard(None).ok()?;
        let result = match GetClipboardData(can) {
            Ok(h) if !h.is_invalid() => {
                let g = windows::Win32::Foundation::HGLOBAL(h.0);
                let p = GlobalLock(g) as *const i32;
                let private = p.is_null() || *p == 0;
                let _ = GlobalUnlock(g);
                Some(private)
            }
            // not rendered yet: look again
            _ => None,
        };
        let _ = CloseClipboard();
        result
    }
}

/// Seconds since the last keyboard or mouse input anywhere.
pub fn idle_seconds() -> i64 {
    unsafe {
        let mut lii = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        if !GetLastInputInfo(&mut lii).as_bool() {
            return 0;
        }
        (GetTickCount().wrapping_sub(lii.dwTime) / 1000) as i64
    }
}

/// (battery percent or -1 without a battery, charger plugged in)
pub fn battery() -> (i64, bool) {
    unsafe {
        let mut s = SYSTEM_POWER_STATUS::default();
        if GetSystemPowerStatus(&mut s).is_err() {
            return (-1, true);
        }
        let pct = if (s.BatteryFlag & 128) == 0 && s.BatteryLifePercent <= 100 { s.BatteryLifePercent as i64 } else { -1 };
        (pct, s.ACLineStatus != 0)
    }
}

/// NVIDIA's capture helper, OBS or Streamlabs running (the island then doesn't hide from captures).
pub fn recorder_running() -> bool {
    const RECORDERS: &[&str] = &["nvsphelper64", "nvsphelper", "obs64", "obs32", "obs", "streamlabs obs", "streamlabs desktop", "bdcam", "camrecorder"];
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return false };
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut found = false;
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            let len = e.szExeFile.iter().position(|c| *c == 0).unwrap_or(e.szExeFile.len());
            let name = String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase();
            let stem = name.strip_suffix(".exe").unwrap_or(&name);
            if RECORDERS.contains(&stem) {
                found = true;
                break;
            }
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(HANDLE(snap.0));
        found
    }
}

/// The browser window in front, for "Stay in place": {"x","y","w","h"} or "null".
pub fn browser_json(proc: &str, fullscreen: bool) -> String {
    const BROWSERS: &[&str] = &["chrome", "msedge", "brave", "opera", "vivaldi", "firefox"];
    let h = foreground();
    let mut r = RECT::default();
    if fullscreen || h.is_invalid() || !BROWSERS.contains(&proc.to_lowercase().as_str()) || unsafe { GetWindowRect(h, &mut r) }.is_err() {
        return "null".into();
    }
    format!("{{\"x\":{},\"y\":{},\"w\":{},\"h\":{}}}", r.left, r.top, r.right - r.left, r.bottom - r.top)
}

#[cfg(test)]
mod fullscreen_tests {
    use super::may_hide_island;

    // each case is a bug users met: these must keep the island on screen
    #[test]
    fn work_that_only_looks_full_screen_keeps_the_island() {
        assert!(!may_hide_island("CabinetWClass", "explorer"), "File Explorer in F11");
        assert!(!may_hide_island("XamlWindow", "SnippingTool"), "Win+Shift+S overlay (Windows 11)");
        assert!(!may_hide_island("ApplicationFrameWindow", "ScreenClippingHost"), "screen snip (Windows 10)");
        assert!(!may_hide_island("Progman", "explorer"), "the desktop");
        assert!(!may_hide_island("Shell_TrayWnd", "explorer"), "the taskbar");
    }

    #[test]
    fn real_full_screen_still_hides_it() {
        assert!(may_hide_island("WinUIDesktopWin32WindowClass", "Photos"), "a photo in full screen");
        assert!(may_hide_island("Chrome_WidgetWin_1", "chrome"), "a video in full screen");
        assert!(may_hide_island("UnityWndClass", "game"), "a game");
    }
}
