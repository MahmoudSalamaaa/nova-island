//! The Apps tab (AppWindows.cs): bring an app's open window to the front instead of starting it
//! again, and tell which pinned apps are running (the dot under their icon). Windows are matched to
//! an app by their AppUserModelID (what the taskbar groups by), Store apps through the window inside
//! their ApplicationFrameHost frame, and other apps by the path of their exe. Nothing runs between
//! clicks: the windows are listed only when asked.
use windows::{
    core::{BOOL, HSTRING, PWSTR},
    Win32::{
        Foundation::{CloseHandle, HWND, LPARAM},
        Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED},
        Storage::{EnhancedStorage::PKEY_AppUserModel_ID, FileSystem::GetFullPathNameW, Packaging::Appx::GetApplicationUserModelId},
        System::Com::StructuredStorage::PropVariantToStringAlloc,
        System::Threading::{AttachThreadInput, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION},
        UI::{
            Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow},
            WindowsAndMessaging::*,
        },
    },
};

struct Entry {
    h: HWND,
    cls: String,
    aumid: String,
    proc_aumid: String,
    exe: String,
}

/// The window's own AppUserModelID (Chrome, VS Code, Telegram and others set one).
fn window_aumid(h: HWND) -> String {
    unsafe {
        let Ok(store) = SHGetPropertyStoreForWindow::<IPropertyStore>(h) else { return String::new() };
        let Ok(pv) = store.GetValue(&PKEY_AppUserModel_ID) else { return String::new() };
        // only a string value (VT_LPWSTR), as the C# helper reads it
        if pv.vt().0 != 31 {
            return String::new();
        }
        PropVariantToStringAlloc(&pv).ok().and_then(|p| {
            let s = p.to_string().ok();
            windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as _));
            s
        }).unwrap_or_default()
    }
}

/// A packaged app's ID from its process, and the exe path of any process.
fn process_info(pid: u32) -> (String, String) {
    unsafe {
        let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return (String::new(), String::new()) };
        let mut buf = [0u16; 1024];
        let mut n = buf.len() as u32;
        let exe = if QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n).is_ok() { String::from_utf16_lossy(&buf[..n as usize]) } else { String::new() };
        let mut ab = [0u16; 256];
        let mut len = ab.len() as u32;
        let aumid = if GetApplicationUserModelId(p, &mut len, Some(PWSTR(ab.as_mut_ptr()))).is_ok() {
            let end = ab.iter().position(|c| *c == 0).unwrap_or(ab.len());
            String::from_utf16_lossy(&ab[..end])
        } else {
            String::new()
        };
        let _ = CloseHandle(p);
        (aumid, exe)
    }
}

unsafe extern "system" fn frame_child(c: HWND, l: LPARAM) -> BOOL {
    let pid = &mut *(l.0 as *mut u32);
    let mut cp = 0u32;
    GetWindowThreadProcessId(c, Some(&mut cp));
    if cp != *pid && crate::win::class_of(c) == "Windows.UI.Core.CoreWindow" {
        *pid = cp;
        return false.into();
    }
    true.into()
}

unsafe extern "system" fn top_level(h: HWND, l: LPARAM) -> BOOL {
    let list = &mut *(l.0 as *mut Vec<Entry>);
    if !IsWindowVisible(h).as_bool() || GetWindow(h, GW_OWNER).map(|o| !o.is_invalid()).unwrap_or(false) {
        return true.into();
    }
    if (GetWindowLongW(h, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0) != 0 || GetWindowTextLengthW(h) == 0 {
        return true.into();
    }
    let mut cloaked = 0i32;
    if DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut i32 as *mut _, 4).is_ok() && cloaked != 0 {
        return true.into();
    }
    let cls = crate::win::class_of(h);
    let aumid = window_aumid(h);
    let mut pid = 0u32;
    GetWindowThreadProcessId(h, Some(&mut pid));
    // a Store app's frame belongs to ApplicationFrameHost; the app is the window inside it
    if cls == "ApplicationFrameWindow" {
        let _ = EnumChildWindows(Some(h), Some(frame_child), LPARAM(&mut pid as *mut u32 as isize));
    }
    let (proc_aumid, exe) = process_info(pid);
    list.push(Entry { h, cls, aumid, proc_aumid, exe });
    true.into()
}

/// Top-level windows a user would call open (as the taskbar shows them), front to back.
fn windows_list() -> Vec<Entry> {
    let mut list: Vec<Entry> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(top_level), LPARAM(&mut list as *mut Vec<Entry> as isize));
    }
    list
}

fn same(a: &str, b: &str) -> bool {
    !a.is_empty() && !b.is_empty() && a.to_uppercase() == b.to_uppercase()
}

fn matches(e: &Entry, id: &str, exe: &str) -> bool {
    if same(&e.aumid, id) || same(&e.proc_aumid, id) {
        return true;
    }
    // File Explorer: its folder windows only, not the desktop or the taskbar
    if same(id, "Microsoft.Windows.Explorer") {
        return e.cls == "CabinetWClass";
    }
    // otherwise by the exe: a window without an ID of its own, or another profile of the same
    // browser ("Chrome.Profile 2"), but never a web app installed from it ("Chrome._crx_...")
    if exe.is_empty() || !same(&e.exe, exe) {
        return false;
    }
    if e.aumid.is_empty() {
        return true;
    }
    let (a, prefix) = (e.aumid.to_uppercase(), format!("{}.", id.to_uppercase()));
    a.starts_with(&prefix) && !a.contains("_CRX_")
}

/// Path.GetFullPath: an absolute, normalised path ("" when it can't be made).
fn full_exe(exe: &str) -> String {
    if exe.is_empty() {
        return String::new();
    }
    let mut buf = [0u16; 1024];
    let n = unsafe { GetFullPathNameW(&HSTRING::from(exe), Some(&mut buf), None) } as usize;
    if n == 0 || n > buf.len() { String::new() } else { String::from_utf16_lossy(&buf[..n]) }
}

fn activate(h: HWND) {
    unsafe {
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        let fg = GetForegroundWindow();
        let fg_thread = if fg.is_invalid() { 0 } else { GetWindowThreadProcessId(fg, None) };
        let me = GetCurrentThreadId();
        // Windows lets the thread that owns the keyboard focus hand it on; share its input for a moment
        let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, true).as_bool();
        let _ = BringWindowToTop(h);
        let _ = SetForegroundWindow(h);
        if attached {
            let _ = AttachThreadInput(me, fg_thread, false);
        }
        if GetForegroundWindow() != h {
            SwitchToThisWindow(h, true);
        }
    }
}

/// "focused" when one of the app's windows came to the front, "none" when it has none open.
/// Clicking an app whose window is already in front moves on to its next window.
pub fn focus(id: &str, exe: &str) -> &'static str {
    let exe = full_exe(exe);
    let mine: Vec<HWND> = windows_list().into_iter().filter(|e| matches(e, id, &exe)).map(|e| e.h).collect();
    let Some(&first) = mine.first() else { return "none" };
    let mut target = first;
    if target == unsafe { GetForegroundWindow() } {
        if mine.len() == 1 {
            return "focused";
        }
        // the one used longest ago, like the taskbar's cycle
        target = *mine.last().unwrap();
    }
    activate(target);
    "focused"
}

/// Of the given apps (id, exe), the ones with a window open.
pub fn running(apps: &[(String, String)]) -> Vec<String> {
    let wins = windows_list();
    apps.iter().filter(|(id, exe)| { let exe = full_exe(exe); wins.iter().any(|e| matches(e, id, &exe)) }).map(|(id, _)| id.clone()).collect()
}
