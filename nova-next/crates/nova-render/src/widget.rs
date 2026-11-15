//! A desktop widget window, native: a rounded glass card drawn by Direct2D, on the desktop under
//! every window, dragged anywhere and snapped to a grid.
//!   glass     the frame extended over the whole window, so what Direct2D leaves transparent is
//!             clear; the card's frosted wallpaper is drawn by the widget (backdrop.rs), with
//!             smooth corners (no Windows corners, border or acrylic)
//!   desktop   owned by the desktop (Progman) and kept at the bottom of the Z order, so windows
//!             cover it and Win+D (which raises the desktop) shows it
//!   drag      the whole widget is a caption: Windows moves it, pixel by pixel; near another
//!             widget or the screen's edge it snaps (side by side GAP apart, or edges in line),
//!             and its place is reported when the move ends
//!   groups    a layout's widgets move together, as one block (layout.rs places them)
//!   menu      a right click: remove the widget, ungroup or remove its group, open Settings
use std::{cell::RefCell, rc::Rc};

use windows::{
    core::{w, Interface, Result, BOOL},
    Win32::{
        Foundation::*,
        Graphics::{
            Direct2D::{Common::*, *},
            Dwm::*,
            Dxgi::Common::*,
            Gdi::*,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{Controls::MARGINS, HiDpi::*, WindowsAndMessaging::*},
    },
};

use crate::font::Fonts;

/// What a widget draws, at `size` DIPs; `tick` says how soon to draw again (ms).
pub trait Face {
    fn size(&self) -> (f32, f32);
    fn draw(&mut self, ctx: &Ctx) -> Result<()>;
    fn tick(&self) -> u32;
    fn id(&self) -> &str;
    /// A click (not a drag) at (x, y) DIPs; true: draw again (and ask `tick` again).
    fn click(&mut self, _x: f32, _y: f32) -> bool {
        false
    }
    /// It needs to be seen now (an alarm): above the windows until it says no more.
    fn on_top(&self) -> bool {
        false
    }
}

/// The card's corner radius (DIPs).
pub const RADIUS: f32 = 22.0;


/// What a face draws with.
pub struct Ctx<'a> {
    pub rt: &'a ID2D1RenderTarget,
    pub d2d: &'a ID2D1Factory,
    pub fonts: &'a Fonts,
}

/// The space between widgets side by side, and from the screen's edge (DIPs).
pub const GAP: f32 = 18.0;
/// How near (DIPs) a widget must come to an edge or a neighbour to snap to it.
const SNAP: f32 = 12.0;

/// Tests: widgets above every window instead of at the bottom, to photograph them.
pub static TEST_ON_TOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn layer() -> HWND {
    if TEST_ON_TOP.load(std::sync::atomic::Ordering::Relaxed) || INTRO.with(|i| i.get()) { HWND_TOPMOST } else { HWND_BOTTOM }
}
/// One widget's layer: above the windows while its face asks for it.
fn layer_of(h: HWND) -> HWND {
    if with_widget(h, |w| w.face.on_top()).unwrap_or(false) { HWND_TOPMOST } else { layer() }
}
const TIMER_TICK: usize = 1;
const TIMER_INTRO: usize = 2;
/// A widget that just opened shows above the windows this long, so it's seen appearing, then
/// settles on the desktop (behind every window).
const INTRO_MS: u32 = 4000;

pub type OnMoved = Rc<dyn Fn(&str, i32, i32)>;

/// What a widget's right-click menu asks for.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Action {
    Remove,
    Ungroup,
    RemoveGroup,
    Settings,
}
const ACTIONS: [Action; 4] = [Action::Remove, Action::Ungroup, Action::RemoveGroup, Action::Settings];
/// the watcher window runs the menu's choice, after the widget's own message is over
const WM_ACTION: u32 = WM_APP + 41;

struct Widget {
    hwnd: HWND,
    face: Box<dyn Face>,
    rt: Option<ID2D1HwndRenderTarget>,
    on_moved: OnMoved,
    place: (i32, i32),
    /// the layout it's in (its widgets move together; the layout's place is reported by name)
    group: Option<Rc<str>>,
}

/// A widget whose window went with Explorer (its owner): it opens again, in the same place, once
/// Explorer is back.
struct Orphan {
    face: Box<dyn Face>,
    on_moved: OnMoved,
    place: (i32, i32),
    group: Option<Rc<str>>,
}

thread_local! {
    static D2D: RefCell<Option<ID2D1Factory>> = const { RefCell::new(None) };
    static FONTS: RefCell<Option<Fonts>> = const { RefCell::new(None) };
    static WIDGETS: RefCell<Vec<Widget>> = const { RefCell::new(Vec::new()) };
    static ORPHANS: RefCell<Vec<Orphan>> = const { RefCell::new(Vec::new()) };
    static WATCH: std::cell::Cell<Option<HWND>> = const { std::cell::Cell::new(None) };
    /// set while the app closes widgets itself (they don't come back)
    static CLOSING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// widgets that just opened are above the windows for a moment
    static INTRO: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// the menu's words (remove, ungroup, remove group, settings) and what a choice does
    static MENU: RefCell<[String; 4]> = RefCell::new(["Remove from desktop".into(), "Ungroup".into(), "Remove group".into(), "Widget settings…".into()]);
    static ON_ACTION: RefCell<Option<Rc<dyn Fn(&str, Option<&str>, Action)>>> = const { RefCell::new(None) };
    static ASKED: RefCell<Vec<(String, Option<String>, Action)>> = const { RefCell::new(Vec::new()) };
    /// widgets whose paint came while another was painting: painted again after it
    static MISSED: RefCell<Vec<HWND>> = const { RefCell::new(Vec::new()) };
}

/// The right-click menu's words, in the app's language.
pub fn menu_text(t: [&str; 4]) {
    MENU.with(|m| *m.borrow_mut() = t.map(String::from));
}
/// What a menu choice does: (the widget's name, its group, the choice).
pub fn on_action(f: Box<dyn Fn(&str, Option<&str>, Action)>) {
    ON_ACTION.with(|a| *a.borrow_mut() = Some(Rc::from(f)));
}

/// Each widget's name and where its window is now (physical pixels).
pub fn places() -> Vec<(String, i32, i32)> {
    WIDGETS.with(|ws| {
        ws.try_borrow()
            .map(|ws| {
                ws.iter()
                    .map(|w| {
                        let mut r = RECT::default();
                        unsafe {
                            let _ = GetWindowRect(w.hwnd, &mut r);
                        }
                        (w.face.id().to_string(), r.left, r.top)
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// The first clear place for a block of `size` DIPs on the main screen: from its top-right
/// corner (GAP in), down and then leftwards, GAP away from the widgets already there.
pub fn free_spot(size: (f32, f32)) -> (i32, i32) {
    let s = unsafe { GetDpiForSystem() } as f32 / 96.0;
    let (w, h) = ((size.0 * s).round() as i32, (size.1 * s).round() as i32);
    let g = (GAP * s).round() as i32;
    let mut work = RECT::default();
    unsafe {
        let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut work as *mut _ as _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    }
    let taken: Vec<RECT> = rects().into_iter().map(|r| r.1).collect();
    let clear = |x: i32, y: i32| taken.iter().all(|o| x + w + g <= o.left || x >= o.right + g || y + h + g <= o.top || y >= o.bottom + g);
    let step = (8.0 * s).round().max(1.0) as i32;
    let mut x = work.right - g - w;
    while x >= work.left + g {
        let mut y = work.top + g;
        while y + h <= work.bottom - g {
            if clear(x, y) {
                return (x, y);
            }
            y += step;
        }
        x -= step;
    }
    (work.right - g - w, work.top + g)
}

/// (1.8.7) Draws the widgets showing `id` again now (what they show changed: the tasks). They
/// read their file every so often only (tasks: 20 s), so a task added or done waited that long,
/// or until the widget was moved.
pub fn redraw(id: &str) {
    let hs: Vec<HWND> = WIDGETS.with(|ws| ws.try_borrow().map(|ws| ws.iter().filter(|w| w.face.id() == id).map(|w| w.hwnd).collect()).unwrap_or_default());
    for h in hs {
        unsafe {
            let _ = InvalidateRect(Some(h), None, false);
        }
    }
}

/// Draws every widget again now (a setting they show changed).
pub fn redraw_all() {
    let hs: Vec<HWND> = WIDGETS.with(|ws| ws.borrow().iter().map(|w| w.hwnd).collect());
    for h in hs {
        unsafe {
            let _ = InvalidateRect(Some(h), None, false);
        }
    }
}

/// Closes every widget for good.
pub fn close_all() {
    CLOSING.with(|c| c.set(true));
    let hs: Vec<HWND> = WIDGETS.with(|ws| ws.borrow().iter().map(|w| w.hwnd).collect());
    for h in hs {
        unsafe {
            let _ = DestroyWindow(h);
        }
    }
    ORPHANS.with(|o| o.borrow_mut().clear());
    CLOSING.with(|c| c.set(false));
}

/// A hidden window that hears Windows' "TaskbarCreated" (Explorer started again) and brings back
/// the widgets that were closed with the old Explorer.
fn watch() {
    if WATCH.with(|w| w.get()).is_some() {
        return;
    }
    unsafe {
        let Ok(inst) = GetModuleHandleW(None) else { return };
        let cls = WNDCLASSW { lpfnWndProc: Some(watch_proc), hInstance: inst.into(), lpszClassName: w!("NovaWidgetWatch"), ..Default::default() };
        RegisterClassW(&cls);
        // top level (not message-only): only top-level windows hear the broadcast
        if let Ok(h) = CreateWindowExW(WS_EX_TOOLWINDOW, w!("NovaWidgetWatch"), w!(""), WS_POPUP, 0, 0, 0, 0, None, None, Some(inst.into()), None) {
            WATCH.with(|w| w.set(Some(h)));
        }
    }
}

extern "system" fn watch_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    static TASKBAR_CREATED: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    let tc = *TASKBAR_CREATED.get_or_init(|| unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) });
    if m == WM_ACTION {
        let asked: Vec<(String, Option<String>, Action)> = ASKED.with(|a| a.borrow_mut().drain(..).collect());
        if let Some(f) = ON_ACTION.with(|a| a.borrow().clone()) {
            for (id, group, what) in asked {
                f(&id, group.as_deref(), what);
            }
        }
        return LRESULT(0);
    }
    if m == tc && tc != 0 {
        let back: Vec<Orphan> = ORPHANS.with(|o| o.borrow_mut().drain(..).collect());
        for o in back {
            let _ = create(o.face, Some(o.place), o.on_moved, o.group);
        }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(h, m, wp, lp) }
}

fn scale_of(hwnd: HWND) -> f32 {
    unsafe { GetDpiForWindow(hwnd) as f32 / 96.0 }
}

/// Opens a widget at `pos` (physical pixels; None: near the top-left of the main screen).
pub fn open(face: Box<dyn Face>, pos: Option<(i32, i32)>, on_moved: Box<dyn Fn(&str, i32, i32)>) -> Result<HWND> {
    create(face, pos, Rc::from(on_moved), None)
}

/// Opens a layout's widgets: each at `origin` (physical pixels) plus its offset (DIPs, at the
/// main screen's scale). They move as one; `on_moved` hears `name` and the layout's top-left.
pub fn open_group(name: &str, faces: Vec<(Box<dyn Face>, (f32, f32))>, origin: (i32, i32), on_moved: Box<dyn Fn(&str, i32, i32)>) -> Result<Vec<HWND>> {
    let s = unsafe { GetDpiForSystem() } as f32 / 96.0;
    let on: OnMoved = Rc::from(on_moved);
    let name: Rc<str> = Rc::from(name);
    faces
        .into_iter()
        .map(|(f, (dx, dy))| create(f, Some((origin.0 + (dx * s).round() as i32, origin.1 + (dy * s).round() as i32)), on.clone(), Some(name.clone())))
        .collect()
}

/// Where the widgets are now (each one's window, its layout).
fn rects() -> Vec<(HWND, RECT, Option<Rc<str>>)> {
    WIDGETS.with(|ws| {
        ws.try_borrow()
            .map(|ws| {
                ws.iter()
                    .map(|w| {
                        let mut r = RECT::default();
                        unsafe {
                            let _ = GetWindowRect(w.hwnd, &mut r);
                        }
                        (w.hwnd, r, w.group.clone())
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}

fn union(rs: impl Iterator<Item = RECT>) -> Option<RECT> {
    rs.reduce(|a, b| RECT { left: a.left.min(b.left), top: a.top.min(b.top), right: a.right.max(b.right), bottom: a.bottom.max(b.bottom) })
}

/// The nudge (dx, dy) that snaps `r` to the screen's edges (GAP in) or to a neighbour (side by
/// side GAP apart, or an edge in line with one of its edges), if one is within SNAP.
pub fn snap(r: RECT, others: &[RECT], work: RECT, s: f32) -> (i32, i32) {
    let (t, g) = ((SNAP * s).round() as i32, (GAP * s).round() as i32);
    let best = |cands: Vec<i32>| cands.into_iter().filter(|d| d.abs() <= t).min_by_key(|d| d.abs()).unwrap_or(0);
    let mut xs = vec![work.left + g - r.left, work.right - g - r.right];
    let mut ys = vec![work.top + g - r.top, work.bottom - g - r.bottom];
    for o in others {
        // in line with its edges, wherever it is
        xs.extend([o.left - r.left, o.right - r.right]);
        ys.extend([o.top - r.top, o.bottom - r.bottom]);
        // beside it (the two overlap, or nearly, the other way)
        if r.top < o.bottom + t && r.bottom > o.top - t {
            xs.extend([o.right + g - r.left, o.left - g - r.right]);
        }
        if r.left < o.right + t && r.right > o.left - t {
            ys.extend([o.bottom + g - r.top, o.top - g - r.bottom]);
        }
    }
    (best(xs), best(ys))
}

/// The nudge that takes `r` off the widgets it covers, to the nearest clear place beside them
/// (GAP apart), kept on the screen.
pub fn unoverlap(r: RECT, others: &[RECT], work: RECT, s: f32) -> (i32, i32) {
    let g = (GAP * s).round() as i32;
    let (mut dx, mut dy) = (0, 0);
    for _ in 0..6 {
        let b = RECT { left: r.left + dx, top: r.top + dy, right: r.right + dx, bottom: r.bottom + dy };
        let Some(o) = others.iter().find(|o| b.left < o.right && b.right > o.left && b.top < o.bottom && b.bottom > o.top) else { break };
        let fits = |x: i32, y: i32| b.left + x >= work.left && b.right + x <= work.right && b.top + y >= work.top && b.bottom + y <= work.bottom;
        let moves = [(o.left - g - b.right, 0), (o.right + g - b.left, 0), (0, o.top - g - b.bottom), (0, o.bottom + g - b.top)];
        let Some(m) = moves.iter().filter(|m| fits(m.0, m.1)).min_by_key(|m| m.0.abs() + m.1.abs()) else { break };
        dx += m.0;
        dy += m.1;
    }
    (dx, dy)
}

fn work_area(hwnd: HWND) -> RECT {
    unsafe {
        let m = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(m, &mut mi);
        mi.rcWork
    }
}

fn create(face: Box<dyn Face>, pos: Option<(i32, i32)>, on_moved: OnMoved, group: Option<Rc<str>>) -> Result<HWND> {
    unsafe {
        D2D.with(|d| -> Result<()> {
            if d.borrow().is_none() {
                *d.borrow_mut() = Some(D2D1CreateFactory::<ID2D1Factory>(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?);
            }
            Ok(())
        })?;
        FONTS.with(|f| -> Result<()> {
            if f.borrow().is_none() {
                *f.borrow_mut() = Some(Fonts::new()?);
            }
            Ok(())
        })?;
        let inst = GetModuleHandleW(None)?;
        let cls = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: inst.into(), lpszClassName: w!("NovaWidget"), hCursor: LoadCursorW(None, IDC_SIZEALL)?, ..Default::default() };
        RegisterClassW(&cls);
        // the desktop owns it: it stays with the desktop when Win+D brings the desktop up
        let desktop = FindWindowW(w!("Progman"), None).ok();
        let (x, y) = pos.unwrap_or((40, 60));
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("NovaWidget"),
            w!("Nova widget"),
            WS_POPUP,
            x,
            y,
            10,
            10,
            desktop,
            None,
            Some(inst.into()),
            None,
        )?;
        // glass: acrylic behind the whole client area, dark, with Windows' rounded corners
        let margins = MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        let dark: BOOL = true.into();
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, 4);
        // the glass is drawn by the widget (backdrop.rs): Windows' acrylic would frost the whole
        // square window. No corners or border from Windows either: the card has its own.
        let corner = DWMWCP_DONOTROUND;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner as *const _ as _, 4);
        let none: u32 = 0xFFFF_FFFE; // DWMWA_COLOR_NONE
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &none as *const _ as _, 4);
        INTRO.with(|i| i.set(true));
        let s = scale_of(hwnd);
        let (w, h) = face.size();
        let _ = SetWindowPos(hwnd, Some(layer()), x, y, (w * s).round() as i32, (h * s).round() as i32, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        let tick = face.tick();
        WIDGETS.with(|ws| ws.borrow_mut().push(Widget { hwnd, face, rt: None, on_moved, place: (x, y), group }));
        watch();
        SetTimer(Some(hwnd), TIMER_TICK, tick, None);
        SetTimer(Some(hwnd), TIMER_INTRO, INTRO_MS, None);
        let _ = InvalidateRect(Some(hwnd), None, false);
        Ok(hwnd)
    }
}

fn with_widget<R>(hwnd: HWND, f: impl FnOnce(&mut Widget) -> R) -> Option<R> {
    WIDGETS.with(|ws| ws.try_borrow_mut().ok().and_then(|mut ws| ws.iter_mut().find(|w| w.hwnd == hwnd).map(f)))
}

fn paint(hwnd: HWND) {
    if crate::backdrop::prepare(hwnd) {
        // the glass is ready: the widgets that painted before it paint again
        for (h, _, _) in rects() {
            if h != hwnd {
                unsafe {
                    let _ = InvalidateRect(Some(h), None, false);
                }
            }
        }
    }
    let done = with_widget(hwnd, |wd| -> Result<()> {
        unsafe {
            let d2d = D2D.with(|d| d.borrow().clone()).expect("D2D");
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc)?;
            let size = D2D_SIZE_U { width: (rc.right - rc.left) as u32, height: (rc.bottom - rc.top) as u32 };
            if wd.rt.is_none() {
                let props = D2D1_RENDER_TARGET_PROPERTIES {
                    r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE, // a widget changes once a minute: no GPU driver loaded (about 20 MB less)
                    pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                    ..Default::default()
                };
                let hp = D2D1_HWND_RENDER_TARGET_PROPERTIES { hwnd, pixelSize: size, presentOptions: D2D1_PRESENT_OPTIONS_NONE };
                wd.rt = Some(d2d.CreateHwndRenderTarget(&props, &hp)?);
            }
            let rt = wd.rt.clone().unwrap();
            let dpi = scale_of(hwnd) * 96.0;
            rt.SetDpi(dpi, dpi);
            rt.Resize(&size)?;
            rt.BeginDraw();
            rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            // the frosted wallpaper in the card's shape (or Nova's dark glass, solid, without one)
            let (w, hgt) = wd.face.size();
            let card = D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: 0.0, top: 0.0, right: w, bottom: hgt }, radiusX: RADIUS, radiusY: RADIUS };
            if !crate::backdrop::fill(&rt, hwnd, dpi / 96.0, &card).unwrap_or(false) {
                let solid = rt.CreateSolidColorBrush(&D2D1_COLOR_F { r: 0.13, g: 0.15, b: 0.24, a: 0.92 }, None)?;
                rt.FillRoundedRectangle(&card, &solid);
            }
            let r = FONTS.with(|f| {
                let fonts = f.borrow();
                let base: ID2D1RenderTarget = rt.cast()?;
                let ctx = Ctx { rt: &base, d2d: &d2d, fonts: fonts.as_ref().unwrap() };
                wd.face.draw(&ctx)
            });
            let end = rt.EndDraw(None, None);
            if end.is_err() {
                wd.rt = None; // the device was lost: a new target next time
            }
            r
        }
    });
    if done.is_none() {
        // another widget was painting (a paint inside a paint): this one paints after it
        MISSED.with(|m| m.borrow_mut().push(hwnd));
    } else {
        let missed: Vec<HWND> = MISSED.with(|m| m.borrow_mut().drain(..).collect());
        for h in missed {
            unsafe {
                let _ = InvalidateRect(Some(h), None, false);
            }
        }
    }
}

extern "system" fn wndproc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match m {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                BeginPaint(h, &mut ps);
                paint(h);
                let _ = EndPaint(h, &ps);
                return LRESULT(0);
            }
            WM_ERASEBKGND => return LRESULT(1),
            // the glass follows the widget as it moves
            WM_MOVE => {
                let _ = InvalidateRect(Some(h), None, false);
            }
            // a new wallpaper, or the screens changed: the glass is made again
            WM_SETTINGCHANGE | WM_DISPLAYCHANGE => {
                crate::backdrop::forget();
                let hs: Vec<HWND> = WIDGETS.with(|ws| ws.try_borrow().map(|ws| ws.iter().map(|w| w.hwnd).collect()).unwrap_or_default());
                for x in hs {
                    let _ = InvalidateRect(Some(x), None, false);
                }
            }
            // the whole widget moves it
            WM_NCHITTEST => return LRESULT(HTCAPTION as isize),
            WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
            // a press: Windows' move loop runs until the button is up; if the widget didn't move it
            // was a click, for the face (a double click is two clicks, never "maximize")
            WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK if wp.0 == HTCAPTION as usize => {
                let mut before = RECT::default();
                let _ = GetWindowRect(h, &mut before);
                if m == WM_NCLBUTTONDOWN {
                    DefWindowProcW(h, m, wp, lp);
                }
                let mut after = RECT::default();
                let _ = GetWindowRect(h, &mut after);
                if before == after {
                    // where the press was (the message's own point: the cursor may be elsewhere by now)
                    let mut p = POINT { x: (lp.0 & 0xffff) as i16 as i32, y: ((lp.0 >> 16) & 0xffff) as i16 as i32 };
                    let _ = ScreenToClient(h, &mut p);
                    let s = scale_of(h);
                    if let Some(Some(_)) = with_widget(h, |wd| wd.face.click(p.x as f32 / s, p.y as f32 / s).then_some(())) {
                        // drawn now, and its timer and layer as the face wants them now
                        SetTimer(Some(h), TIMER_TICK, 1, None);
                    }
                }
                return LRESULT(0);
            }
            // a right click: the widget's menu (instead of Windows' window menu)
            WM_NCRBUTTONDOWN | WM_CONTEXTMENU => return LRESULT(0),
            WM_NCRBUTTONUP => {
                let Some((id, group)) = with_widget(h, |wd| (wd.face.id().to_string(), wd.group.as_ref().map(|g| g.to_string()))) else { return LRESULT(0) };
                let words = MENU.with(|m| m.borrow().clone());
                let rtl = words[0].chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c));
                if let Ok(menu) = CreatePopupMenu() {
                    let item = |n: usize| {
                        let t: Vec<u16> = words[n].encode_utf16().chain([0]).collect();
                        let _ = AppendMenuW(menu, MF_STRING, n + 1, windows::core::PCWSTR(t.as_ptr()));
                    };
                    item(0);
                    if group.is_some() {
                        item(1);
                        item(2);
                    }
                    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
                    item(3);
                    let p = POINT { x: (lp.0 & 0xffff) as i16 as i32, y: ((lp.0 >> 16) & 0xffff) as i16 as i32 };
                    // the menu closes on a click elsewhere only while its window is in front
                    let _ = SetForegroundWindow(h);
                    let flags = TPM_RETURNCMD | TPM_RIGHTBUTTON | if rtl { TPM_LAYOUTRTL | TPM_RIGHTALIGN } else { TPM_LEFTALIGN };
                    let cmd = TrackPopupMenu(menu, flags, p.x, p.y, Some(0), h, None).0 as usize;
                    let _ = DestroyMenu(menu);
                    if (1..=4).contains(&cmd) {
                        ASKED.with(|a| a.borrow_mut().push((id, group, ACTIONS[cmd - 1])));
                        if let Some(wh) = WATCH.with(|w| w.get()) {
                            let _ = PostMessageW(Some(wh), WM_ACTION, WPARAM(0), LPARAM(0));
                        }
                    }
                }
                return LRESULT(0);
            }
            // always at the bottom: windows cover it, it never covers them
            WM_WINDOWPOSCHANGING => {
                let p = &mut *(lp.0 as *mut WINDOWPOS);
                if p.flags.0 & SWP_NOZORDER.0 == 0 {
                    p.hwndInsertAfter = layer_of(h);
                }
            }
            // it moves freely; near an edge or a neighbour it snaps; a layout moves as one block
            WM_MOVING => {
                let r = &mut *(lp.0 as *mut RECT);
                let all = rects();
                let Some((_, now, group)) = all.iter().find(|w| w.0 == h).cloned() else { return LRESULT(1) };
                let same = |g: &Option<Rc<str>>| group.is_some() && g == &group;
                let (dx, dy) = (r.left - now.left, r.top - now.top);
                // what moves: this widget, or its whole layout
                let block = if group.is_some() { union(all.iter().filter(|w| same(&w.2)).map(|w| w.1)).unwrap_or(now) } else { now };
                let moved = RECT { left: block.left + dx, top: block.top + dy, right: block.right + dx, bottom: block.bottom + dy };
                let others: Vec<RECT> = all.iter().filter(|w| w.0 != h && !same(&w.2)).map(|w| w.1).collect();
                let (sx, sy) = snap(moved, &others, work_area(h), scale_of(h));
                r.left += sx;
                r.right += sx;
                r.top += sy;
                r.bottom += sy;
                // the rest of the layout, by the same amount, together
                if group.is_some() {
                    let (mx, my) = (r.left - now.left, r.top - now.top);
                    let mates: Vec<(HWND, RECT)> = all.iter().filter(|w| w.0 != h && same(&w.2)).map(|w| (w.0, w.1)).collect();
                    if let Ok(mut dwp) = BeginDeferWindowPos(mates.len() as i32) {
                        for (mh, mr) in &mates {
                            if let Ok(next) = DeferWindowPos(dwp, *mh, None, mr.left + mx, mr.top + my, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE) {
                                dwp = next;
                            }
                        }
                        let _ = EndDeferWindowPos(dwp);
                    }
                }
                return LRESULT(1);
            }
            WM_EXITSIZEMOVE => {
                // let go over another widget: to the nearest clear place beside it
                let all = rects();
                if let Some((_, _, group)) = all.iter().find(|w| w.0 == h).cloned() {
                    let mine = |w: &(HWND, RECT, Option<Rc<str>>)| w.0 == h || (group.is_some() && w.2 == group);
                    if let Some(block) = union(all.iter().filter(|w| mine(w)).map(|w| w.1)) {
                        let others: Vec<RECT> = all.iter().filter(|w| !mine(w)).map(|w| w.1).collect();
                        let (dx, dy) = unoverlap(block, &others, work_area(h), scale_of(h));
                        if (dx, dy) != (0, 0) {
                            for w in all.iter().filter(|w| mine(w)) {
                                let _ = SetWindowPos(w.0, None, w.1.left + dx, w.1.top + dy, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
                            }
                        }
                    }
                }
                let all = rects();
                let group = all.iter().find(|w| w.0 == h).and_then(|w| w.2.clone());
                for (wh, wr, _) in &all {
                    let _ = with_widget(*wh, |wd| wd.place = (wr.left, wr.top));
                }
                match group {
                    Some(name) => {
                        let block = union(all.iter().filter(|w| w.2.as_ref() == Some(&name)).map(|w| w.1));
                        if let (Some(b), Some(on)) = (block, with_widget(h, |wd| wd.on_moved.clone())) {
                            on(&name, b.left, b.top);
                        }
                    }
                    None => {
                        if let Some((on, id, r)) = with_widget(h, |wd| (wd.on_moved.clone(), wd.face.id().to_string(), wd.place)) {
                            on(&id, r.0, r.1);
                        }
                    }
                }
                let _ = SetWindowPos(h, Some(layer()), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
            // another screen scale: Windows' suggested rectangle, at the widget's own size
            WM_DPICHANGED => {
                let r = &*(lp.0 as *const RECT);
                let s = (wp.0 & 0xffff) as f32 / 96.0;
                let size = with_widget(h, |wd| wd.face.size()).unwrap_or((160.0, 160.0));
                let _ = SetWindowPos(h, None, r.left, r.top, (size.0 * s).round() as i32, (size.1 * s).round() as i32, SWP_NOZORDER | SWP_NOACTIVATE);
                return LRESULT(0);
            }
            // the moment above the windows is over: down to the desktop
            WM_TIMER if wp.0 == TIMER_INTRO => {
                let _ = KillTimer(Some(h), TIMER_INTRO);
                INTRO.with(|i| i.set(false));
                let _ = SetWindowPos(h, Some(HWND(-2isize as _)), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                let _ = SetWindowPos(h, Some(layer()), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                return LRESULT(0);
            }
            WM_TIMER if wp.0 == TIMER_TICK => {
                let _ = InvalidateRect(Some(h), None, false);
                let _ = UpdateWindow(h);
                if let Some(t) = with_widget(h, |wd| wd.face.tick()) {
                    SetTimer(Some(h), TIMER_TICK, t, None);
                    let top = layer_of(h) == HWND_TOPMOST;
                    // an alarm came (or went): above the windows, or back down to the desktop
                    let is_top = GetWindowLongW(h, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0 != 0;
                    if top != is_top {
                        if !top {
                            let _ = SetWindowPos(h, Some(HWND(-2isize as _)), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                        }
                        let _ = SetWindowPos(h, Some(layer_of(h)), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                    }
                }
                return LRESULT(0);
            }
            WM_DESTROY => {
                // gone with Explorer (not closed by the app): kept to open again when it's back
                let gone = WIDGETS.with(|ws| {
                    let mut ws = ws.borrow_mut();
                    let i = ws.iter().position(|w| w.hwnd == h)?;
                    Some(ws.remove(i))
                });
                if let Some(w) = gone {
                    if !CLOSING.with(|c| c.get()) {
                        ORPHANS.with(|o| o.borrow_mut().push(Orphan { face: w.face, on_moved: w.on_moved, place: w.place, group: w.group }));
                    }
                }
            }
            _ => {}
        }
        DefWindowProcW(h, m, wp, lp)
    }
}

/// Per-monitor DPI for the process (once, before any window).
pub fn dpi_aware() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn r(l: i32, t: i32, w: i32, h: i32) -> RECT {
        RECT { left: l, top: t, right: l + w, bottom: t + h }
    }
    #[test]
    fn snaps_beside_and_in_line() {
        let work = r(0, 0, 1920, 1040);
        let other = r(1000, 100, 176, 176);
        // 7 px short of the gap on the left side, 5 px off its top: both snap
        assert_eq!(snap(r(1000 - 18 - 176 - 7, 105, 176, 176), &[other], work, 1.0), (7, -5));
        // far from everything: free
        assert_eq!(snap(r(500, 500, 176, 176), &[other], work, 1.0), (0, 0));
        // near the screen's right edge: GAP in
        assert_eq!(snap(r(1920 - 18 - 176 + 6, 500, 176, 176), &[], work, 1.0), (-6, 0));
    }
    #[test]
    fn let_go_on_another_moves_beside_it() {
        let work = r(0, 0, 1920, 1040);
        let other = r(1000, 100, 176, 176);
        // covering its bottom 26 px: goes under it, GAP apart
        assert_eq!(unoverlap(r(1000, 250, 370, 176), &[other], work, 1.0), (0, 44));
        assert_eq!(unoverlap(r(10, 10, 100, 100), &[other], work, 1.0), (0, 0));
    }
}
