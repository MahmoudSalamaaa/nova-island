// The app's normal windows: Settings (prefs-window.js) and, new in 1.8, the Radio window. Each is a
// focusable window, frameless with the page's own title bar, made when opened and destroyed when
// closed, so it costs nothing while closed. Settings runs the unchanged prefs/index.html with the
// original prefs/preload.js; Radio runs 1.8's radio/index.html.
use std::{cell::RefCell, rc::Rc};

use nova_web::{Bridge, Page};
use serde_json::{json, Value};
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use windows::{
    core::{w, Interface, PCWSTR},
    Win32::{
        Foundation::*,
        Graphics::Gdi::{CreateSolidBrush, HBRUSH},
        System::LibraryLoader::GetModuleHandleW,
        UI::{HiDpi::GetDpiForWindow, WindowsAndMessaging::*},
    },
};

use crate::{app, router::Src, App};

/// Events the Settings page listens to (besides "features", sent raw from features_changed).
pub const EVENTS: &[&str] = &["general", "location", "calendar", "plugins-list", "plugin-progress", "show-page", "update", "update-check"];

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Settings,
    Radio,
    Tasks,
}

struct Spec {
    class: PCWSTR,
    title: PCWSTR,
    page: &'static str,
    bridge: Bridge,
    src: Src,
    size: (i32, i32),
    min: (i32, i32),
    log: &'static str,
}

fn spec(k: Kind) -> Spec {
    match k {
        Kind::Settings => Spec { class: w!("NovaIslandSettings"), title: w!("Nova Island Settings"), page: "prefs/index.html", bridge: Bridge::Prefs, src: Src::Prefs, size: (760, 560), min: (640, 480), log: "settings" },
        Kind::Radio => Spec { class: w!("NovaIslandRadio"), title: w!("Quran Radio"), page: "radio/index.html", bridge: Bridge::Radio, src: Src::Radio, size: (760, 560), min: (560, 420), log: "radio" },
        Kind::Tasks => Spec { class: w!("NovaIslandTasks"), title: w!("Tasks"), page: "tasks/index.html", bridge: Bridge::Tasks, src: Src::Tasks, size: (980, 640), min: (700, 480), log: "tasks" },
    }
}

pub struct Prefs {
    pub hwnd: HWND,
    pub page: Rc<Page>,
}

fn slot(a: &App, k: Kind) -> &Option<Prefs> {
    match k {
        Kind::Settings => &a.prefs,
        Kind::Radio => &a.radio,
        Kind::Tasks => &a.tasks_win,
    }
}

fn slot_mut(a: &mut App, k: Kind) -> &mut Option<Prefs> {
    match k {
        Kind::Settings => &mut a.prefs,
        Kind::Radio => &mut a.radio,
        Kind::Tasks => &mut a.tasks_win,
    }
}

/// The page of a window, if it's open.
pub fn page(a: &Rc<RefCell<App>>, k: Kind) -> Option<Rc<Page>> {
    slot(&a.borrow(), k).as_ref().map(|p| p.page.clone())
}

fn hwnd_of(a: &Rc<RefCell<App>>, k: Kind) -> Option<HWND> {
    slot(&a.borrow(), k).as_ref().map(|p| p.hwnd)
}

/// Which window `h` is.
fn kind_of(h: HWND) -> Option<Kind> {
    let a = app()?;
    [Kind::Settings, Kind::Radio, Kind::Tasks].into_iter().find(|k| hwnd_of(&a, *k) == Some(h))
}

fn scale(h: HWND) -> f64 {
    unsafe { GetDpiForWindow(h).max(96) as f64 / 96.0 }
}

extern "system" fn wndproc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match m {
            // no system frame: the page draws its own title bar, the window keeps its resize edges
            WM_NCCALCSIZE if wp.0 != 0 => {
                if IsZoomed(h).as_bool() {
                    let p = &mut *(lp.0 as *mut NCCALCSIZE_PARAMS);
                    let b = GetSystemMetrics(SM_CXSIZEFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER);
                    p.rgrc[0].left += b;
                    p.rgrc[0].top += b;
                    p.rgrc[0].right -= b;
                    p.rgrc[0].bottom -= b;
                }
                return LRESULT(0);
            }
            WM_NCHITTEST => {
                let x = (lp.0 & 0xffff) as i16 as i32;
                let y = ((lp.0 >> 16) & 0xffff) as i16 as i32;
                let mut r = RECT::default();
                let _ = GetWindowRect(h, &mut r);
                let e = (6.0 * scale(h)) as i32;
                let (l, t, rr, b) = (x < r.left + e, y < r.top + e, x >= r.right - e, y >= r.bottom - e);
                let hit = match (l, t, rr, b) {
                    (true, true, _, _) => HTTOPLEFT,
                    (_, true, true, _) => HTTOPRIGHT,
                    (true, _, _, true) => HTBOTTOMLEFT,
                    (_, _, true, true) => HTBOTTOMRIGHT,
                    (true, ..) => HTLEFT,
                    (_, true, ..) => HTTOP,
                    (_, _, true, _) => HTRIGHT,
                    (.., true) => HTBOTTOM,
                    _ => HTCLIENT,
                };
                return LRESULT(hit as isize);
            }
            WM_GETMINMAXINFO => {
                if let Some(k) = kind_of(h) {
                    let mm = &mut *(lp.0 as *mut MINMAXINFO);
                    let (s, (mw, mh)) = (scale(h), spec(k).min);
                    mm.ptMinTrackSize = POINT { x: (mw as f64 * s) as i32, y: (mh as f64 * s) as i32 };
                    return LRESULT(0);
                }
            }
            WM_SIZE => {
                if let (Some(a), Some(k)) = (app(), kind_of(h)) {
                    // sent from inside a move while the app is held (a display scaled past 100%):
                    // the page follows a moment later, from the queue (1.8.2)
                    let p = match a.try_borrow() {
                        Ok(s) => slot(&s, k).as_ref().map(|p| p.page.clone()),
                        Err(_) => {
                            let _ = PostMessageW(Some(h), WM_SIZE, WPARAM(0), LPARAM(0));
                            None
                        }
                    };
                    if let Some(p) = p {
                        let mut r = RECT::default();
                        let _ = GetClientRect(h, &mut r);
                        let _ = p.controller.SetBounds(r);
                    }
                }
            }
            WM_DPICHANGED => {
                let r = &*(lp.0 as *const RECT);
                let _ = SetWindowPos(h, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
                return LRESULT(0);
            }
            WM_CLOSE => {
                if let Some(k) = kind_of(h) {
                    close_kind(k);
                }
                return LRESULT(0);
            }
            _ => {}
        }
        DefWindowProcW(h, m, wp, lp)
    }
}

/// Open Settings (on `page` when given: a news link, "Get plugin").
pub fn open(page: Option<String>) {
    open_kind(Kind::Settings, page);
}

/// Open a window. Done from the message loop: a click on the island arrives inside a WebView2
/// event, and a new page can't be made in there.
pub fn open_kind(k: Kind, page: Option<String>) {
    let Some(a) = app() else { return };
    let bus = a.borrow().bus.clone();
    bus.ui(move |_| open_now(k, page));
}

fn open_now(k: Kind, page: Option<String>) {
    let Some(a) = app() else { return };
    if let (Some(h), Some(p)) = (hwnd_of(&a, k), self::page(&a, k)) {
        unsafe {
            if IsIconic(h).as_bool() {
                let _ = ShowWindow(h, SW_RESTORE);
            }
            let _ = ShowWindow(h, SW_SHOW);
            let _ = SetForegroundWindow(h);
        }
        if let Some(pg) = page {
            p.event("show-page", json!(pg));
        }
        return;
    }
    if let Err(e) = create(&a, k, page) {
        nova_core::log::error(&format!("[{}] can't open: {e}", spec(k).log));
    }
}

fn create(a: &Rc<RefCell<App>>, k: Kind, page: Option<String>) -> Result<(), String> {
    let sp = spec(k);
    let env = a.borrow().env.clone().ok_or("no WebView2")?;
    let hwnd = unsafe {
        let inst = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        let cls = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: HINSTANCE(inst.0),
            lpszClassName: sp.class,
            hCursor: LoadCursorW(None, IDC_ARROW).map_err(|e| e.to_string())?,
            hIcon: LoadImageW(Some(HINSTANCE(inst.0)), PCWSTR_ICON, IMAGE_ICON, 0, 0, LR_DEFAULTSIZE).map(|h| HICON(h.0)).unwrap_or_default(),
            // backgroundColor #0f0f15 until the page paints
            hbrBackground: HBRUSH(CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x00150f0f)).0),
            ..Default::default()
        };
        RegisterClassW(&cls);
        // centred on the primary screen, in DIPs
        let d = crate::screen::primary();
        let (w, h) = ((sp.size.0 as f64 * d.scale) as i32, (sp.size.1 as f64 * d.scale) as i32);
        let (cx, cy) = d.to_phys(d.work_area.x + d.work_area.width / 2.0, d.work_area.y + d.work_area.height / 2.0);
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            sp.class,
            sp.title,
            WS_POPUP | WS_THICKFRAME | WS_MINIMIZEBOX | WS_SYSMENU | WS_CAPTION,
            cx - w / 2,
            cy - h / 2,
            w,
            h,
            None,
            None,
            Some(HINSTANCE(inst.0)),
            None,
        )
        .map_err(|e| e.to_string())?
    };
    let mut r = RECT::default();
    unsafe {
        // apply WM_NCCALCSIZE now (no frame), so the page fills the whole window
        let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        let _ = GetClientRect(hwnd, &mut r);
    }
    let src = sp.src;
    let pg = Rc::new(Page::create(&env, hwnd, r.right, r.bottom, Some(sp.bridge), move |msg| {
        if let Some(a) = app() {
            crate::router::handle(&a, msg, src);
        }
    })?);
    pg.set_background(0x0f, 0x0f, 0x15, 255)?;
    unsafe {
        // the page's title bar (-webkit-app-region: drag) moves the window
        if let Ok(s) = pg.webview.Settings().and_then(|s| s.cast::<ICoreWebView2Settings9>()) {
            let _ = s.SetIsNonClientRegionSupportEnabled(true);
        }
    }
    let log = sp.log;
    pg.on_console(move |line| nova_core::log::error(&format!("[{log}] {line}")))?;
    // (1.8.4) the language goes with the address, so the page is in Arabic from its first paint
    // instead of drawing English while it asks for the settings
    let lang = if a.borrow().core.features.get()["prefsLang"] == "ar" { "?lang=ar" } else { "" };
    pg.navigate(&format!("{}{}{lang}", nova_web::ORIGIN, sp.page))?;
    *slot_mut(&mut a.borrow_mut(), k) = Some(Prefs { hwnd, page: pg.clone() });
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
    if let Some(p) = page {
        // after the page has set up its listeners
        let pg2 = pg.clone();
        let mut token = 0;
        unsafe {
            let _ = pg.webview.add_NavigationCompleted(
                &webview2_com::NavigationCompletedEventHandler::create(Box::new(move |_, _| {
                    pg2.event("show-page", json!(p));
                    Ok(())
                })),
                &mut token,
            );
        }
    }
    Ok(())
}

/// Close Settings and free its page.
pub fn close() {
    close_kind(Kind::Settings);
}

pub fn close_kind(k: Kind) {
    let Some(a) = app() else { return };
    let p = slot_mut(&mut a.borrow_mut(), k).take();
    if let Some(p) = p {
        unsafe {
            let _ = p.page.controller.Close();
            let _ = DestroyWindow(p.hwnd);
        }
    }
}

pub fn minimize() {
    minimize_kind(Kind::Settings);
}

pub fn minimize_kind(k: Kind) {
    let Some(a) = app() else { return };
    if let Some(h) = hwnd_of(&a, k) {
        unsafe {
            let _ = ShowWindow(h, SW_MINIMIZE);
        }
    }
}

/// Send to the Settings page, if it's open.
pub fn send(a: &Rc<RefCell<App>>, channel: &str, data: Value) {
    send_kind(a, Kind::Settings, channel, data);
}

pub fn send_kind(a: &Rc<RefCell<App>>, k: Kind, channel: &str, data: Value) {
    if let Some(p) = page(a, k) {
        p.event(channel, data);
    }
}

const PCWSTR_ICON: PCWSTR = PCWSTR(1 as *const u16);
