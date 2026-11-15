//! (1.8.7) Where on the taskbar the Nova strip may sit (Settings › Show on the taskbar). Kept
//! apart from the drawing.
//!
//! - Windows 11 with the icons centred: from the Widgets (weather) button's right edge, or the
//!   taskbar's left, to the first centred icon (Start).
//! - Windows 10, or icons on the left: right-aligned against the notification area.
//!
//! The taskbar's buttons are found with UI Automation on a thread of their own, every few seconds
//! and only while the strip is on (WANTED).
use std::{
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Duration,
};

use windows::{
    core::{BSTR, HSTRING, PCWSTR},
    Win32::{
        Foundation::RECT,
        System::{
            Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED},
            Registry::{RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ},
            Variant::VARIANT,
        },
        UI::{
            Accessibility::*,
            HiDpi::GetDpiForWindow,
            Shell::{SHAppBarMessage, ABM_GETSTATE, APPBARDATA},
            WindowsAndMessaging::{FindWindowExW, FindWindowW, GetWindowRect, IsWindowVisible},
        },
    },
};

/// Something needs the slot (the strip is on).
pub static WANTED: AtomicBool = AtomicBool::new(false);

/// The free part of the primary taskbar, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    pub taskbar: RECT,
    /// the strip goes between these (x)
    pub x_min: i32,
    pub x_max: i32,
    /// true: against x_max (beside the tray); false: against x_min (after the weather)
    pub at_end: bool,
    /// the taskbar's DPI
    pub dpi: u32,
    /// Windows' taskbar in its light theme
    pub light: bool,
}

fn reg_dword(root: HKEY, key: &str, name: &str) -> Option<u32> {
    let mut v = 0u32;
    let mut size = 4u32;
    unsafe { RegGetValueW(root, &HSTRING::from(key), &HSTRING::from(name), RRF_RT_REG_DWORD, None, Some(&mut v as *mut u32 as *mut _), Some(&mut size)) }.ok().ok()?;
    Some(v)
}

fn windows_build() -> u32 {
    let mut buf = [0u16; 32];
    let mut size = (buf.len() * 2) as u32;
    let ok = unsafe { RegGetValueW(HKEY_LOCAL_MACHINE, &HSTRING::from(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"), &HSTRING::from("CurrentBuildNumber"), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size)) }.is_ok();
    if !ok {
        return 0;
    }
    String::from_utf16_lossy(&buf).trim_end_matches('\0').parse().unwrap_or(0)
}

fn element_rect(uia: &IUIAutomation, root: &IUIAutomationElement, id: &str) -> Option<RECT> {
    unsafe {
        let cond = uia.CreatePropertyCondition(UIA_AutomationIdPropertyId, &VARIANT::from(BSTR::from(id))).ok()?;
        let e = root.FindFirst(TreeScope_Descendants, &cond).ok()?;
        let r = e.CurrentBoundingRectangle().ok()?;
        (r.right > r.left).then_some(r)
    }
}

/// The slot now, or None when there's no taskbar to sit on (hidden, auto-hide, on a side).
pub fn find(uia: Option<&IUIAutomation>) -> Option<Slot> {
    unsafe {
        let tray = FindWindowW(&HSTRING::from("Shell_TrayWnd"), PCWSTR::null()).ok()?;
        if !IsWindowVisible(tray).as_bool() {
            return None;
        }
        let mut tb = RECT::default();
        GetWindowRect(tray, &mut tb).ok()?;
        // a taskbar on a screen's side: not in this version
        if tb.bottom - tb.top > tb.right - tb.left {
            return None;
        }
        // auto-hide: the strip would float over the desktop when the taskbar slides away
        let mut abd = APPBARDATA { cbSize: std::mem::size_of::<APPBARDATA>() as u32, hWnd: tray, ..Default::default() };
        if SHAppBarMessage(ABM_GETSTATE, &mut abd) & 1 != 0 {
            return None;
        }
        let dpi = GetDpiForWindow(tray).max(96);
        let light = reg_dword(HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "SystemUsesLightTheme") == Some(1);
        let pad = (6 * dpi / 96) as i32;
        let win11 = windows_build() >= 22000;
        let centred = win11 && reg_dword(HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "TaskbarAl") != Some(0);
        if centred {
            if let Some(uia) = uia {
                if let Ok(root) = uia.ElementFromHandle(tray) {
                    let start = element_rect(uia, &root, "StartButton");
                    let widgets = element_rect(uia, &root, "WidgetsButton");
                    if let Some(s) = start {
                        // no weather button: a clear gap from the screen's edge, not glued to it
                        let edge = (16 * dpi / 96) as i32;
                        let x_min = widgets.map(|w| w.right + pad).unwrap_or(tb.left + edge);
                        return Some(Slot { taskbar: tb, x_min, x_max: s.left - pad, at_end: false, dpi, light });
                    }
                }
            }
            return None;
        }
        // icons on the left: against the notification area
        let notify = FindWindowExW(Some(tray), None, &HSTRING::from("TrayNotifyWnd"), PCWSTR::null()).ok();
        let mut nr = RECT::default();
        let end = match notify {
            Some(n) if GetWindowRect(n, &mut nr).is_ok() && nr.right > nr.left => nr.left,
            _ => tb.right - (200 * dpi / 96) as i32,
        };
        Some(Slot { taskbar: tb, x_min: tb.left + (300 * dpi / 96) as i32, x_max: end - pad, at_end: true, dpi, light })
    }
}

/// The slot, kept current on its own thread while WANTED (every 3 s: centred icons move as apps
/// open and close; UI Automation isn't asked when the strip is off).
pub fn spawn() -> Arc<Mutex<Option<Slot>>> {
    let latest = Arc::new(Mutex::new(None));
    let out = latest.clone();
    std::thread::Builder::new()
        .name("nova-taskbar".into())
        .spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            let uia: Option<IUIAutomation> = unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok() };
            if let Some(u2) = uia.as_ref().and_then(|u| windows::core::Interface::cast::<IUIAutomation2>(u).ok()) {
                unsafe {
                    let _ = u2.SetConnectionTimeout(1000);
                    let _ = u2.SetTransactionTimeout(1000);
                }
            }
            loop {
                if WANTED.load(std::sync::atomic::Ordering::Relaxed) {
                    let s = find(uia.as_ref());
                    *out.lock().unwrap() = s;
                    std::thread::sleep(Duration::from_secs(3));
                } else {
                    *out.lock().unwrap() = None;
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        })
        .expect("nova-taskbar thread");
    latest
}
