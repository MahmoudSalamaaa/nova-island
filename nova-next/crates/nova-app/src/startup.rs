// One running copy, and "Start with Windows" (nova.js): the same Run value 1.7 writes
// ("electron.app.Nova Island"), so turning it on or off in either version means the same thing.
// At every start an entry pointing at an old path is moved to this exe, keeping it disabled if the
// user turned it off in Task Manager (the 1.7 fix, item 19). A dev build never touches the registry.
use windows::{
    core::HSTRING,
    Win32::{
        Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HWND},
        System::{Registry::*, Threading::CreateMutexW},
    },
};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const NAME: &str = "electron.app.Nova Island";

/// False when another copy is already running (it keeps the island; this one quits).
pub fn first_instance(tag: &str) -> bool {
    unsafe {
        let _ = CreateMutexW(None, false, &HSTRING::from(format!("Local\\com.dynamicnova.app.{tag}")));
        GetLastError() != ERROR_ALREADY_EXISTS
    }
}

/// The running copy's island window (to bring it forward for a second launch).
///
/// (1.8.7) The copy of this same exe: any island window was taken before, so a test build could
/// talk to the installed app, or the GitHub one to the Store one (each holds its own mutex).
/// Only this exe's own copy is shown, or ended when it's stuck.
pub fn other_window() -> Option<HWND> {
    use windows::Win32::{
        Foundation::{CloseHandle, LPARAM},
        System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION},
        UI::WindowsAndMessaging::{EnumWindows, GetClassNameW, GetWindowThreadProcessId},
    };
    struct Find {
        me: String,
        found: Option<HWND>,
    }
    unsafe extern "system" fn each(h: HWND, l: LPARAM) -> windows::core::BOOL {
        let f = &mut *(l.0 as *mut Find);
        let mut cls = [0u16; 64];
        let n = GetClassNameW(h, &mut cls) as usize;
        let want: Vec<u16> = crate::island::CLASS.as_wide().to_vec();
        if cls[..n] != want[..] {
            return true.into();
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid));
        if pid == std::process::id() {
            return true.into();
        }
        if let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
            let _ = CloseHandle(p);
            if ok && String::from_utf16_lossy(&buf[..len as usize]).eq_ignore_ascii_case(&f.me) {
                f.found = Some(h);
                return false.into();
            }
        }
        true.into()
    }
    let mut f = Find { me: exe(), found: None };
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut f as *mut Find as isize));
    }
    f.found
}

/// Only an installed build changes the registry.
pub fn installed() -> bool {
    !cfg!(debug_assertions) && !dev_build()
}

/// A copy run from where Cargo built it (any target folder: its exe sits next to "deps"), not an
/// installed one: it keeps to the test data and never touches the registry. An installed copy
/// never has a "deps" folder beside it.
pub fn dev_build() -> bool {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().contains("nova-next-target") || p.parent().is_some_and(|d| d.join("deps").is_dir()))
        .unwrap_or(true)
}

fn exe() -> String {
    std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
}

fn read(key: &str, name: &str) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; 2048];
    let mut size = buf.len() as u32;
    unsafe { RegGetValueW(HKEY_CURRENT_USER, &HSTRING::from(key), &HSTRING::from(name), RRF_RT_ANY, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size)) }.ok().ok()?;
    buf.truncate(size as usize);
    Some(buf)
}

fn run_value() -> Option<String> {
    let b = read(RUN, NAME)?;
    let w: Vec<u16> = b.chunks(2).map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)])).take_while(|c| *c != 0).collect();
    Some(String::from_utf16_lossy(&w))
}

fn target(cmd: &str) -> &str {
    match cmd.strip_prefix('"') {
        Some(r) => r.split('"').next().unwrap_or(""),
        None => cmd.split(' ').next().unwrap_or(""),
    }
}

/// Starts with Windows: the Run value points at this exe and Task Manager hasn't disabled it. The
/// Store version starts through its package (the "SlackStartup" task), managed in Windows' Startup
/// apps, so it counts as on (nova.js: STORE).
pub fn starts_with_windows() -> bool {
    if store() {
        return true;
    }
    let on = run_value().is_some_and(|c| target(&c).eq_ignore_ascii_case(&exe()));
    let disabled = read(APPROVED, NAME).is_some_and(|b| b.first() == Some(&3));
    on && !disabled
}

pub fn set_start_with_windows(on: bool) {
    if !installed() || store() {
        return;
    }
    unsafe {
        let mut key = HKEY::default();
        if RegCreateKeyExW(HKEY_CURRENT_USER, &HSTRING::from(RUN), None, None, REG_OPTION_NON_VOLATILE, KEY_SET_VALUE, None, &mut key, None).is_err() {
            return;
        }
        if on {
            let v: Vec<u16> = format!("\"{}\"", exe()).encode_utf16().chain([0]).collect();
            let bytes = std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 2);
            let _ = RegSetValueExW(key, &HSTRING::from(NAME), None, REG_SZ, Some(bytes));
        } else {
            let _ = RegDeleteValueW(key, &HSTRING::from(NAME));
        }
        let _ = RegCloseKey(key);
    }
}

/// The 1.7 fix: an entry left pointing at an old install folder follows this exe.
pub fn refresh() {
    if !installed() || store() {
        return;
    }
    let Some(cmd) = run_value() else { return };
    let old = target(&cmd).to_string();
    if old.is_empty() || old.eq_ignore_ascii_case(&exe()) {
        return;
    }
    set_start_with_windows(true);
    let disabled = read(APPROVED, NAME).is_some_and(|b| b.first() == Some(&3));
    nova_core::log::info(&format!("start with Windows: moved the entry from {old} to {}{}", exe(), if disabled { " (kept disabled)" } else { "" }));
}

/// Running as the Microsoft Store package (Windows then manages startup and updates).
pub fn store() -> bool {
    static STORE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *STORE.get_or_init(|| {
        let mut len = 0u32;
        // APPMODEL_ERROR_NO_PACKAGE (15700) when the app has no package identity
        unsafe { windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName(&mut len, None).0 != 15700 }
    })
}
