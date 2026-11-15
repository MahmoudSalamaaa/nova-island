//! (1.8.7) The Nova strip on the taskbar (Settings › Show on the taskbar). Display only: the
//! window lets every click through. Drawn with Direct2D into a layered window: no WebView2. This
//! file knows nothing about where the information comes from (nova-app/src/taskbar.rs decides it).
//!
//!   [ (••) | content ]     Nova's eyes always first, then what matters now
use std::cell::RefCell;

use windows_numerics::{Matrix3x2, Vector2};

use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM},
        Graphics::{
            Direct2D::{Common::*, *},
            DirectWrite::*,
            Gdi::*,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    },
};

/// What the strip says.
#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    /// CPU and RAM, percent
    Perf { cpu: u32, ram: u32 },
    /// the next prayer within two hours: "العصر بعد 25 د", and how much of the two hours has gone
    Prayer { text: String, fill: f32 },
    /// the prayer has come: "حيّ على الفلاح" (30 s)
    Hayya { text: String },
    /// media playing: its title and the time it has reached; a video shows ▶ instead of the bars
    Media { title: String, time: String, video: bool, playing: bool },
}

/// What Nova's eyes do (the waves take their place while media plays).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mood {
    /// awake: blinking, and now and then a look left and right
    Normal,
    /// asleep: shut, with a "z" rising (midnight to 5 in the morning)
    Sleep,
    /// a cup of coffee in their place, steaming (the day's first use of the PC, 10 min)
    Coffee,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// Windows' taskbar in its light theme
    pub light: bool,
    /// the island theme's accent (0..1)
    pub accent: (f32, f32, f32),
    /// Settings › Remove animations
    pub still: bool,
}

/// Where: the free part of the taskbar, physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub x_min: i32,
    pub x_max: i32,
    pub at_end: bool,
    pub top: i32,
    pub bottom: i32,
    pub dpi: u32,
}

const TIMER_EQ: usize = 1;
const TIMER_BLINK: usize = 2;
/// the z rising, the steam, and a look left and right while it lasts
const TIMER_ANIM: usize = 3;
/// when the next look left and right starts
const TIMER_LOOK: usize = 4;
/// a look left and right takes this long
const LOOK_S: f32 = 1.6;

struct Strip {
    hwnd: HWND,
    d2d: ID2D1Factory,
    dw: IDWriteFactory,
    content: Content,
    look: Look,
    place: Place,
    phase: u32,
    eyes_shut: bool,
    eq_on: bool,
    mood: Mood,
    /// a look left and right under way: since when
    looking: Option<std::time::Instant>,
    /// the z's / the steam's beat
    tick: u32,
    anim_ms: u32,
    /// where the strip itself was drawn last (physical pixels): the card goes over it
    at: (i32, i32),
    /// a reminder card over the strip: what, since when
    card: Option<(Card, std::time::Instant)>,
    card_hwnd: Option<HWND>,
    /// the Qur'an font (KFGQPC Hafs, from Nova's own files), made once
    quran: Option<Option<(IDWriteFontCollection, Vec<u16>)>>,
}

/// (1.8.7) A reminder card over the strip, in Madar's look (Settings › Show on the taskbar: the
/// island's verse / dhikr / du'a card comes here instead). Display only, like the strip.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    /// "آية", "ذكر", "دعاء"
    pub kind: String,
    pub text: String,
    /// "سورة الرعد · ٢٨" (may be empty)
    pub reference: String,
    /// a verse: in ﴿ ﴾ with its number as the verse-end sign
    pub quran: bool,
    pub ms: u32,
}

thread_local! {
    static QURAN_BYTES: std::cell::Cell<Option<&'static [u8]>> = const { std::cell::Cell::new(None) };
}

/// The Qur'an font's file (nova-app gives it from the embedded pages' fonts).
pub fn set_quran_font(bytes: &'static [u8]) {
    QURAN_BYTES.with(|b| b.set(Some(bytes)));
}

const TIMER_CARD: usize = 5;
const CARD_W: f32 = 340.0;

thread_local! {
    static STRIP: RefCell<Option<Strip>> = const { RefCell::new(None) };
}

fn rgba(r: f32, g: f32, b: f32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r, g, b, a }
}

fn has_arabic(s: &str) -> bool {
    s.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c))
}

unsafe extern "system" fn wndproc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match m {
        WM_TIMER => {
            STRIP.with(|s| {
                let Ok(mut s) = s.try_borrow_mut() else { return };
                let Some(st) = s.as_mut() else { return };
                match w.0 {
                    TIMER_EQ => st.phase = st.phase.wrapping_add(1),
                    TIMER_CARD => {
                        let over = st.card.as_ref().is_none_or(|(c, t)| t.elapsed().as_millis() as u32 >= c.ms);
                        if over {
                            st.card = None;
                            unsafe {
                                let _ = KillTimer(Some(h), TIMER_CARD);
                                if let Some(ch) = st.card_hwnd {
                                    let _ = ShowWindow(ch, SW_HIDE);
                                }
                            }
                        } else {
                            draw_card(st);
                            // quick while it comes in, then four times a second for its bar
                            let fresh = st.card.as_ref().is_some_and(|(_, t)| t.elapsed().as_millis() < 220);
                            unsafe {
                                SetTimer(Some(h), TIMER_CARD, if fresh { 30 } else { 250 }, None);
                            }
                        }
                    }
                    TIMER_ANIM => {
                        st.tick = st.tick.wrapping_add(1);
                        if st.looking.is_some_and(|t| t.elapsed().as_secs_f32() > LOOK_S) {
                            st.looking = None;
                        }
                    }
                    TIMER_LOOK => {
                        // every 50 to 70 s, awake and with nothing else in the eyes' place
                        if st.mood == Mood::Normal && !st.look.still && !matches!(st.content, Content::Media { .. }) {
                            st.looking = Some(std::time::Instant::now());
                        }
                        unsafe {
                            SetTimer(Some(h), TIMER_LOOK, 50_000 + (st.phase.wrapping_mul(7919) % 20_000), None);
                        }
                    }
                    TIMER_BLINK => {
                        st.eyes_shut = !st.eyes_shut;
                        // shut a moment, open for 4 to 7 s
                        let next = if st.eyes_shut { 140 } else { 4000 + (st.phase.wrapping_mul(997) % 3000) };
                        unsafe {
                            SetTimer(Some(h), TIMER_BLINK, next, None);
                        }
                    }
                    _ => {}
                }
                pace(st);
                draw(st);
            });
            LRESULT(0)
        }
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        _ => DefWindowProcW(h, m, w, l),
    }
}

fn create() -> Option<Strip> {
    unsafe {
        let inst = GetModuleHandleW(None).ok()?;
        let class = w!("NovaTaskbarStrip");
        let wc = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: inst.into(), lpszClassName: class, ..Default::default() };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            class,
            w!("Nova"),
            WS_POPUP,
            0,
            0,
            10,
            10,
            None,
            None,
            Some(inst.into()),
            None,
        )
        .ok()?;
        let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).ok()?;
        let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()?;
        SetTimer(Some(hwnd), TIMER_BLINK, 4000, None);
        SetTimer(Some(hwnd), TIMER_LOOK, 45_000, None);
        Some(Strip { hwnd, d2d, dw, content: Content::Perf { cpu: 0, ram: 0 }, look: Look { light: false, accent: (0.55, 0.78, 1.0), still: false }, place: Place { x_min: 0, x_max: 0, at_end: false, top: 0, bottom: 0, dpi: 96 }, phase: 0, eyes_shut: false, eq_on: false, mood: Mood::Normal, looking: None, tick: 0, anim_ms: 0, at: (0, 0), card: None, card_hwnd: None, quran: None })
    }
}

/// The animation timer at the pace what shows needs, or none (most of the time: still eyes).
fn pace(st: &mut Strip) {
    let media = matches!(st.content, Content::Media { .. });
    let ms = if st.look.still || media {
        0
    } else if st.looking.is_some() {
        50
    } else if matches!(st.mood, Mood::Sleep | Mood::Coffee) {
        200
    } else {
        0
    };
    if ms != st.anim_ms {
        st.anim_ms = ms;
        unsafe {
            if ms > 0 {
                SetTimer(Some(st.hwnd), TIMER_ANIM, ms, None);
            } else {
                let _ = KillTimer(Some(st.hwnd), TIMER_ANIM);
            }
        }
    }
}

/// Shows (or updates) the strip.
pub fn show(place: &Place, content: &Content, look: &Look, mood: Mood) {
    STRIP.with(|s| {
        let Ok(mut s) = s.try_borrow_mut() else { return };
        if s.is_none() {
            *s = create();
        }
        let Some(st) = s.as_mut() else { return };
        let same = st.content == *content && st.look == *look && st.place == *place && st.mood == mood && unsafe { IsWindowVisible(st.hwnd).as_bool() };
        st.mood = mood;
        st.content = content.clone();
        st.look = *look;
        st.place = *place;
        // the bars move only while something plays (and animations are on)
        let eq = matches!(content, Content::Media { playing: true, .. }) && !look.still;
        if eq != st.eq_on {
            st.eq_on = eq;
            unsafe {
                if eq {
                    SetTimer(Some(st.hwnd), TIMER_EQ, 120, None);
                } else {
                    let _ = KillTimer(Some(st.hwnd), TIMER_EQ);
                }
            }
        }
        if look.still {
            st.eyes_shut = false;
            st.looking = None;
        }
        pace(st);
        // (while something moves, its own timer draws)
        if !same && (!eq || st.anim_ms == 0) {
            draw(st);
        }
        // above the taskbar (a click on it brings the taskbar forward)
        unsafe {
            let _ = SetWindowPos(st.hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    });
}

pub fn hide() {
    STRIP.with(|s| {
        if let Ok(s) = s.try_borrow() {
            if let Some(st) = s.as_ref() {
                unsafe {
                    let _ = ShowWindow(st.hwnd, SW_HIDE);
                }
            }
        }
    });
}

pub fn close() {
    STRIP.with(|s| {
        if let Ok(mut s) = s.try_borrow_mut() {
            if let Some(st) = s.take() {
                unsafe {
                    let _ = DestroyWindow(st.hwnd);
                    if let Some(c) = st.card_hwnd {
                        let _ = DestroyWindow(c);
                    }
                }
            }
        }
    });
}

struct Text {
    layout: IDWriteTextLayout,
    w: f32,
    h: f32,
}

/// `steady`: as wide as the same text with every digit an 8 at least ("53%" as "88%", "4:17" as
/// "8:88"), so a number changing doesn't make the strip grow and shrink; only a longer number
/// (100%) makes it wider.
fn text(dw: &IDWriteFactory, s: &str, size: f32, bold: bool, max_w: f32) -> Option<Text> {
    text_steady(dw, s, size, bold, max_w, false)
}

fn text_steady(dw: &IDWriteFactory, s: &str, size: f32, bold: bool, max_w: f32, steady: bool) -> Option<Text> {
    let least = if steady { text_steady(dw, &s.chars().map(|c| if c.is_ascii_digit() { '8' } else { c }).collect::<String>(), size, bold, max_w, false).map(|t| t.w) } else { None };
    let mut t = text_inner(dw, s, size, bold, max_w)?;
    if let Some(l) = least {
        t.w = t.w.max(l);
    }
    Some(t)
}

fn text_inner(dw: &IDWriteFactory, s: &str, size: f32, bold: bool, max_w: f32) -> Option<Text> {
    unsafe {
        let weight = if bold { DWRITE_FONT_WEIGHT_SEMI_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL };
        let f = dw.CreateTextFormat(w!("Segoe UI"), None, weight, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!("")).ok()?;
        let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
        if has_arabic(s) {
            let _ = f.SetReadingDirection(DWRITE_READING_DIRECTION_RIGHT_TO_LEFT);
        }
        let ellipsis = dw.CreateEllipsisTrimmingSign(&f).ok();
        let trim = DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 };
        let _ = f.SetTrimming(&trim, ellipsis.as_ref());
        let wide: Vec<u16> = s.encode_utf16().collect();
        let layout = dw.CreateTextLayout(&wide, &f, max_w, 100.0).ok()?;
        let mut m = DWRITE_TEXT_METRICS::default();
        layout.GetMetrics(&mut m).ok()?;
        let w = m.widthIncludingTrailingWhitespace.min(max_w);
        let _ = layout.SetMaxWidth(w.ceil() + 1.0);
        Some(Text { layout, w: w.ceil() + 1.0, h: m.height })
    }
}

/// One piece of the content, left to right.
enum Piece {
    Text(Text, D2D1_COLOR_F),
    /// a text centred in the width it's given (`t.w`), its own width the second value
    Centre(Text, f32, D2D1_COLOR_F),
    Sep,
}

/// How wide the performance content is with two-digit values: the prayer takes the same width,
/// so the strip keeps its size as the two take turns.
fn perf_width(dw: &IDWriteFactory) -> f32 {
    let w = |s: &str, size: f32, bold: bool| text(dw, s, size, bold, 60.0).map(|t| t.w).unwrap_or(0.0);
    let (cpu, ram, pct) = (w("CPU", 11.0, false), w("RAM", 11.0, false), w("88%", 12.0, true));
    // CPU 88% | RAM 88%: five pieces, four gaps
    cpu + pct + 1.0 + ram + pct + 4.0 * 8.0
}

fn draw(st: &mut Strip) {
    let p = st.place;
    let k = p.dpi as f32 / 96.0;
    let light = st.look.light;
    let (ar, ag, ab) = st.look.accent;
    let ink = if light { rgba(0.05, 0.05, 0.07, 0.9) } else { rgba(1.0, 1.0, 1.0, 0.93) };
    let soft = if light { rgba(0.05, 0.05, 0.07, 0.56) } else { rgba(1.0, 1.0, 1.0, 0.62) };
    let line = if light { rgba(0.0, 0.0, 0.0, 0.16) } else { rgba(1.0, 1.0, 1.0, 0.2) };
    let seg = if light { rgba(0.0, 0.0, 0.0, 0.05) } else { rgba(1.0, 1.0, 1.0, 0.07) };
    let amber_ink = if light { rgba(0.52, 0.31, 0.04, 1.0) } else { rgba(0.98, 0.8, 0.45, 1.0) };
    let accent_ink = if light { rgba(ar * 0.55, ag * 0.55, ab * 0.55, 1.0) } else { rgba(ar, ag, ab, 1.0) };
    // the room there is, in DIPs; the strip's height follows the taskbar
    let room = ((p.x_max - p.x_min) as f32 / k).max(0.0);
    let h = (((p.bottom - p.top) as f32 / k) - 12.0).clamp(24.0, 34.0);
    const PAD: f32 = 10.0;
    const GAP: f32 = 8.0;
    const EYES: f32 = 13.0;
    let fixed = PAD + EYES + GAP + 1.0 + GAP + PAD;
    if room < PAD * 2.0 + EYES {
        hide_window(st.hwnd);
        return;
    }
    let avail = room - fixed;
    let dw = st.dw.clone();
    let mut fill: Option<(f32, D2D1_COLOR_F)> = None;
    let mut pieces: Vec<Piece> = Vec::new();
    match &st.content {
        Content::Perf { cpu, ram } => {
            for (i, (label, v)) in [("CPU", *cpu), ("RAM", *ram)].into_iter().enumerate() {
                if i > 0 {
                    pieces.push(Piece::Sep);
                }
                if let Some(t) = text(&dw, label, 11.0, false, 60.0) {
                    pieces.push(Piece::Text(t, soft));
                }
                // never narrower than two digits ("5%" as wide as "88%"); 100% is wider
                if let Some(mut t) = text(&dw, &format!("{v}%"), 12.0, true, 60.0) {
                    if let Some(two) = text(&dw, "88%", 12.0, true, 60.0) {
                        t.w = t.w.max(two.w);
                    }
                    pieces.push(Piece::Text(t, ink));
                }
            }
        }
        Content::Prayer { text: s, fill: f } => {
            fill = Some((f.clamp(0.0, 1.0), rgba(0.93, 0.62, 0.15, if light { 0.22 } else { 0.28 })));
            if let Some(mut t) = text(&dw, s, 12.0, true, avail.max(40.0)) {
                let own = t.w;
                t.w = t.w.max(perf_width(&dw).min(avail));
                pieces.push(Piece::Centre(t, own, amber_ink));
            }
        }
        Content::Hayya { text: s } => {
            fill = Some((1.0, rgba(ar, ag, ab, if light { 0.25 } else { 0.3 })));
            if let Some(mut t) = text(&dw, s, 12.0, true, avail.max(40.0)) {
                let own = t.w;
                t.w = t.w.max(perf_width(&dw).min(avail));
                pieces.push(Piece::Centre(t, own, accent_ink));
            }
        }
        // (the waves take the eyes' place; nothing that looks like a button: the strip only shows)
        Content::Media { title, time, .. } => {
            let tt = text_steady(&dw, time, 12.0, false, 60.0, true);
            let time_w = tt.as_ref().map(|t| t.w).unwrap_or(0.0);
            let title_max = (avail - GAP - 1.0 - time_w).clamp(0.0, 170.0);
            if title_max >= 24.0 {
                if let Some(t) = text(&dw, title, 12.0, false, title_max) {
                    pieces.push(Piece::Text(t, ink));
                }
            }
            pieces.push(Piece::Sep);
            if let Some(t) = tt {
                pieces.push(Piece::Text(t, soft));
            }
        }
    }
    let piece_w = |p: &Piece| match p {
        Piece::Text(t, _) | Piece::Centre(t, _, _) => t.w,
        Piece::Sep => 1.0,
    };
    let mut content_w: f32 = pieces.iter().map(piece_w).sum::<f32>() + GAP * pieces.len().saturating_sub(1) as f32;
    // not enough room for the content: the eyes alone
    if content_w > avail {
        pieces.clear();
        content_w = 0.0;
    }
    let width = if pieces.is_empty() { PAD * 2.0 + EYES } else { fixed + content_w };
    let (pw, ph) = ((width * k).ceil() as i32, (h * k).ceil() as i32);
    let x = if p.at_end { p.x_max - pw } else { p.x_min };
    let y = p.top + ((p.bottom - p.top) - ph) / 2;
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: pw, biHeight: -ph, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() }, ..Default::default() };
        let mut bits = std::ptr::null_mut();
        let Ok(bmp) = CreateDIBSection(Some(mem), &bi, DIB_RGB_COLORS, &mut bits, None, 0) else {
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
            return;
        };
        let old = SelectObject(mem, bmp.into());
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT { format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
            ..Default::default()
        };
        if let Ok(rt) = st.d2d.CreateDCRenderTarget(&props) {
            let rc = RECT { left: 0, top: 0, right: pw, bottom: ph };
            if rt.BindDC(mem, &rc).is_ok() {
                rt.SetDpi(p.dpi as f32, p.dpi as f32);
                rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
                rt.BeginDraw();
                rt.Clear(Some(&rgba(0.0, 0.0, 0.0, 0.0)));
                rt.SetTransform(&Matrix3x2::identity());
                let brush = |c: D2D1_COLOR_F| rt.CreateSolidColorBrush(&c, None).ok();
                let card = D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: 0.0, top: 0.0, right: width, bottom: h }, radiusX: 9.0, radiusY: 9.0 };
                if let Some(b) = brush(seg) {
                    rt.FillRoundedRectangle(&card, &b);
                }
                // the prayer's two hours filling from the start
                if let Some((f, c)) = fill {
                    if f > 0.0 {
                        let r = D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: 0.0, top: 0.0, right: (width * f).max(18.0), bottom: h }, radiusX: 9.0, radiusY: 9.0 };
                        if let Some(b) = brush(c) {
                            rt.FillRoundedRectangle(&r, &b);
                        }
                    }
                }
                // Nova's eyes, or the waves in their place while media plays
                let cy = h / 2.0;
                if matches!(st.content, Content::Media { .. }) {
                    if let Some(b) = brush(accent_ink) {
                        for i in 0..3u32 {
                            // each bar its own beat; paused or still: a calm shape
                            let t = if st.eq_on { (st.phase + i * 3) % 8 } else { [3, 6, 4][i as usize] };
                            let bh = 3.0 + (if t < 4 { t } else { 8 - t }) as f32 * 2.4;
                            let bx = PAD + i as f32 * 4.5;
                            let r = D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: bx, top: cy - bh / 2.0, right: bx + 3.0, bottom: cy + bh / 2.0 }, radiusX: 1.5, radiusY: 1.5 };
                            rt.FillRoundedRectangle(&r, &b);
                        }
                    }
                } else if st.mood == Mood::Coffee {
                    // a cup and its handle, steam rising over it
                    if let Some(b) = brush(ink) {
                        let cup = D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: PAD, top: cy - 2.0, right: PAD + 9.0, bottom: cy + 5.5 }, radiusX: 1.5, radiusY: 1.5 };
                        rt.FillRoundedRectangle(&cup, &b);
                        let handle = D2D1_ELLIPSE { point: Vector2 { X: PAD + 9.5, Y: cy + 1.5 }, radiusX: 2.2, radiusY: 2.2 };
                        rt.DrawEllipse(&handle, &b, 1.3, None);
                    }
                    for i in 0..2u32 {
                        let t = ((st.tick + i * 3) % 6) as f32 / 6.0;
                        let a = if st.look.still { 0.5 } else { (1.0 - (t * 2.0 - 1.0).abs()) * 0.8 };
                        if let Some(b) = brush(D2D1_COLOR_F { a: soft.a * a, ..soft }) {
                            let sx = PAD + 2.5 + i as f32 * 4.0;
                            let y0 = cy - 4.0 - t * 4.0;
                            rt.DrawLine(Vector2 { X: sx, Y: y0 }, Vector2 { X: sx + 0.8, Y: y0 - 3.0 }, &b, 1.2, None);
                        }
                    }
                } else {
                    let sleep = st.mood == Mood::Sleep;
                    // reading the card over the strip: half shut, looking up
                    let reading = st.card.is_some() && !sleep;
                    let eh = if sleep || st.eyes_shut { 1.6 } else if reading { 5.0 } else { 8.0 };
                    let cy = if reading { cy - 2.0 } else { cy };
                    // a look left and right: there, hold, the other side, hold, back
                    let dx = st.looking.map(|t| {
                        let s = t.elapsed().as_secs_f32();
                        let ease = |a: f32, b: f32, v: f32| ((s - a) / (b - a)).clamp(0.0, 1.0) * v;
                        if s < 0.8 { ease(0.0, 0.3, -2.5) } else if s < 1.4 { -2.5 + ease(0.8, 1.1, 5.0) } else { 2.5 - ease(1.4, LOOK_S, 2.5) }
                    }).unwrap_or(0.0);
                    if let Some(b) = brush(ink) {
                        for i in 0..2 {
                            let ex = PAD + i as f32 * 8.0 + dx;
                            let r = D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: ex, top: cy - eh / 2.0, right: ex + 5.0, bottom: cy + eh / 2.0 }, radiusX: 2.5, radiusY: 2.5_f32.min(eh / 2.0) };
                            rt.FillRoundedRectangle(&r, &b);
                        }
                    }
                    // asleep: small z's rising from the eyes (still: one, where it is)
                    if sleep {
                        if let Some(zt) = text(&dw, "z", 9.0, true, 20.0) {
                            for i in 0..2u32 {
                                let t = if st.look.still { 0.5 } else { ((st.tick + i * 5) % 10) as f32 / 10.0 };
                                let a = (1.0 - (t * 2.0 - 1.0).abs()).max(0.15);
                                if let Some(b) = brush(D2D1_COLOR_F { a: soft.a * a, ..soft }) {
                                    rt.DrawTextLayout(Vector2 { X: PAD + EYES - 2.0 + t * 4.0, Y: cy - 3.0 - t * 8.0 - zt.h / 2.0 }, &zt.layout, &b, D2D1_DRAW_TEXT_OPTIONS_NONE);
                                }
                                if st.look.still {
                                    break;
                                }
                            }
                        }
                    }
                }
                if !pieces.is_empty() {
                    let mut x = PAD + EYES + GAP;
                    if let Some(b) = brush(line) {
                        rt.DrawLine(Vector2 { X: x + 0.5, Y: cy - 9.0 }, Vector2 { X: x + 0.5, Y: cy + 9.0 }, &b, 1.0 / k, None);
                    }
                    x += 1.0 + GAP;
                    for piece in &pieces {
                        match piece {
                            Piece::Text(t, c) => {
                                if let Some(b) = brush(*c) {
                                    rt.DrawTextLayout(Vector2 { X: x, Y: cy - t.h / 2.0 }, &t.layout, &b, D2D1_DRAW_TEXT_OPTIONS_NONE);
                                }
                            }
                            Piece::Centre(t, own, c) => {
                                if let Some(b) = brush(*c) {
                                    rt.DrawTextLayout(Vector2 { X: x + (t.w - own) / 2.0, Y: cy - t.h / 2.0 }, &t.layout, &b, D2D1_DRAW_TEXT_OPTIONS_NONE);
                                }
                            }
                            Piece::Sep => {
                                if let Some(b) = brush(line) {
                                    rt.DrawLine(Vector2 { X: x + 0.5, Y: cy - 8.0 }, Vector2 { X: x + 0.5, Y: cy + 8.0 }, &b, 1.0 / k, None);
                                }
                            }
                        }
                        x += piece_w(piece) + GAP;
                    }
                }
                let _ = rt.EndDraw(None, None);
            }
        }
        st.at = (x, y);
        let pt = POINT { x, y };
        let sz = SIZE { cx: pw, cy: ph };
        let src = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
        let _ = UpdateLayeredWindow(st.hwnd, Some(screen), Some(&pt), Some(&sz), Some(mem), Some(&src), COLORREF(0), Some(&blend), ULW_ALPHA);
        SelectObject(mem, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        if !IsWindowVisible(st.hwnd).as_bool() {
            let _ = ShowWindow(st.hwnd, SW_SHOWNOACTIVATE);
        }
    }
    let _ = PCWSTR::null();
}

fn hide_window(h: HWND) {
    unsafe {
        let _ = ShowWindow(h, SW_HIDE);
    }
}

/// Shows a reminder card over the strip for `card.ms`. False when there's no strip showing (the
/// island shows it then).
pub fn card(card: &Card) -> bool {
    STRIP.with(|s| {
        let Ok(mut s) = s.try_borrow_mut() else { return false };
        let Some(st) = s.as_mut() else { return false };
        if !unsafe { IsWindowVisible(st.hwnd).as_bool() } {
            return false;
        }
        st.card = Some((card.clone(), std::time::Instant::now()));
        draw_card(st);
        draw(st);
        unsafe {
            SetTimer(Some(st.hwnd), TIMER_CARD, 30, None);
        }
        true
    })
}

fn quran_font(dw: &IDWriteFactory) -> Option<(IDWriteFontCollection, Vec<u16>)> {
    use windows::core::Interface;
    let bytes = QURAN_BYTES.with(|b| b.get())?;
    unsafe {
        let f5: IDWriteFactory5 = dw.cast().ok()?;
        let loader = f5.CreateInMemoryFontFileLoader().ok()?;
        f5.RegisterFontFileLoader(&loader).ok()?;
        let file = loader.CreateInMemoryFontFileReference(&f5, bytes.as_ptr() as *const _, bytes.len() as u32, None).ok()?;
        let b = f5.CreateFontSetBuilder().ok()?;
        b.AddFontFile(&file).ok()?;
        let set = b.CreateFontSet().ok()?;
        let coll = f5.CreateFontCollectionFromFontSet(&set).ok()?;
        let fam = coll.GetFontFamily(0).ok()?;
        let names = fam.GetFamilyNames().ok()?;
        let mut buf = [0u16; 128];
        names.GetString(0, &mut buf).ok()?;
        let n = buf.iter().position(|c| *c == 0).unwrap_or(0);
        Some((coll.cast().ok()?, buf[..=n].to_vec()))
    }
}

/// A verse's numbers as the verse-end sign (U+06DD then the number, one ornament in the font),
/// the verse in ﴿ ﴾ (views/reminders.js does the same for the island).
fn quran_text(t: &str) -> String {
    let words: Vec<String> = t
        .split(' ')
        .map(|w| if !w.is_empty() && w.chars().all(|c| ('\u{0660}'..='\u{0669}').contains(&c)) && w.chars().count() <= 3 { format!("\u{06DD}{w}") } else { w.to_string() })
        .collect();
    format!("\u{FD3F}{}\u{FD3E}", words.join(" "))
}

fn card_text(dw: &IDWriteFactory, font: Option<&(IDWriteFontCollection, Vec<u16>)>, s: &str, size: f32, width: f32) -> Option<Text> {
    unsafe {
        let (family, coll) = match font {
            Some((c, f)) => (PCWSTR(f.as_ptr()), Some(c)),
            None => (w!("Segoe UI"), None),
        };
        let f = dw.CreateTextFormat(family, coll, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!("ar")).ok()?;
        let _ = f.SetReadingDirection(DWRITE_READING_DIRECTION_RIGHT_TO_LEFT);
        let _ = f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
        let wide: Vec<u16> = s.encode_utf16().collect();
        let layout = dw.CreateTextLayout(&wide, &f, width, 400.0).ok()?;
        let mut m = DWRITE_TEXT_METRICS::default();
        layout.GetMetrics(&mut m).ok()?;
        Some(Text { layout, w: width, h: m.height })
    }
}

fn draw_card(st: &mut Strip) {
    let Some((c, since)) = st.card.clone() else { return };
    let p = st.place;
    let k = p.dpi as f32 / 96.0;
    let dw = st.dw.clone();
    if st.quran.is_none() {
        st.quran = Some(quran_font(&dw));
    }
    let font = st.quran.as_ref().and_then(|q| q.as_ref()).filter(|_| c.quran);
    let (ar, ag, ab) = st.look.accent;
    let mint = rgba(ar, ag, ab, 1.0);
    let ink = rgba(0.95, 0.96, 0.965, 1.0);
    let soft = rgba(0.84, 0.86, 0.89, 0.66);
    // the text's size from its length (marks add characters but no width), as the island does
    let bare = c.text.chars().filter(|ch| !matches!(*ch as u32, 0x064B..=0x065F | 0x0670 | 0x06D6..=0x06ED)).count();
    let mut size = if bare <= 40 { 24.0 } else if bare <= 80 { 21.0 } else { 19.0 };
    if !c.quran {
        size -= 5.0;
    }
    let shown = if c.quran { quran_text(&c.text) } else { c.text.clone() };
    const PADC: f32 = 14.0;
    let inner = CARD_W - PADC * 2.0;
    let mut body = card_text(&dw, font, &shown, size, inner);
    while body.as_ref().is_some_and(|b| b.h > 150.0) && size > 15.0 {
        size -= 2.0;
        body = card_text(&dw, font, &shown, size, inner);
    }
    let Some(body) = body else { return };
    let head = text(&dw, &c.kind, 11.0, true, inner);
    let brand = text(&dw, "Nova", 10.0, false, 60.0);
    let refer = (!c.reference.is_empty()).then(|| text(&dw, &c.reference, 11.0, false, inner)).flatten();
    let head_h = 16.0;
    let ref_h = refer.as_ref().map(|r| r.h + 2.0).unwrap_or(0.0);
    let h = 12.0 + head_h + 4.0 + body.h + 2.0 + ref_h + 8.0 + 3.0 + 12.0;
    let (pw, ph) = ((CARD_W * k).ceil() as i32, (h * k).ceil() as i32);
    // over the strip, its left edge with the strip's, kept on the screen
    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let x = st.at.0.clamp(0, (screen_w - pw).max(0));
    let y = p.top - ph - (8.0 * k) as i32;
    let elapsed = since.elapsed().as_secs_f32();
    let total = c.ms as f32 / 1000.0;
    let alpha = (elapsed / 0.2).min(1.0).min(((total - elapsed) / 0.25).clamp(0.0, 1.0));
    let left = (1.0 - elapsed / total).clamp(0.0, 1.0);
    unsafe {
        let hwnd = match st.card_hwnd {
            Some(hw) => hw,
            None => {
                let inst = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).ok();
                let class = w!("NovaTaskbarCard");
                let wc = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: inst.map(Into::into).unwrap_or_default(), lpszClassName: class, ..Default::default() };
                RegisterClassW(&wc);
                let Ok(hw) = CreateWindowExW(WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST, class, w!("Nova"), WS_POPUP, 0, 0, 10, 10, None, None, inst.map(Into::into), None) else { return };
                st.card_hwnd = Some(hw);
                hw
            }
        };
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: pw, biHeight: -ph, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() }, ..Default::default() };
        let mut bits = std::ptr::null_mut();
        let Ok(bmp) = CreateDIBSection(Some(mem), &bi, DIB_RGB_COLORS, &mut bits, None, 0) else {
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
            return;
        };
        let old = SelectObject(mem, bmp.into());
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT { format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
            ..Default::default()
        };
        if let Ok(rt) = st.d2d.CreateDCRenderTarget(&props) {
            let rc = RECT { left: 0, top: 0, right: pw, bottom: ph };
            if rt.BindDC(mem, &rc).is_ok() {
                rt.SetDpi(p.dpi as f32, p.dpi as f32);
                rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
                rt.BeginDraw();
                rt.Clear(Some(&rgba(0.0, 0.0, 0.0, 0.0)));
                let brush = |c: D2D1_COLOR_F| rt.CreateSolidColorBrush(&c, None).ok();
                // Madar: graphite, a hairline, mint
                let card = D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: 0.5, top: 0.5, right: CARD_W - 0.5, bottom: h - 0.5 }, radiusX: 14.0, radiusY: 14.0 };
                if let Some(b) = brush(rgba(0.114, 0.125, 0.149, 0.97)) {
                    rt.FillRoundedRectangle(&card, &b);
                }
                if let Some(b) = brush(rgba(1.0, 1.0, 1.0, 0.13)) {
                    rt.DrawRoundedRectangle(&card, &b, 1.0, None);
                }
                let mut y0 = 12.0;
                // the kind on the right (Arabic reads from there), Nova on the left
                if let (Some(t), Some(b)) = (&head, brush(mint)) {
                    rt.DrawTextLayout(Vector2 { X: CARD_W - PADC - t.w, Y: y0 }, &t.layout, &b, D2D1_DRAW_TEXT_OPTIONS_NONE);
                }
                if let (Some(t), Some(b)) = (&brand, brush(soft)) {
                    rt.DrawTextLayout(Vector2 { X: PADC, Y: y0 + 1.0 }, &t.layout, &b, D2D1_DRAW_TEXT_OPTIONS_NONE);
                }
                y0 += head_h + 4.0;
                if let Some(b) = brush(ink) {
                    rt.DrawTextLayout(Vector2 { X: PADC, Y: y0 }, &body.layout, &b, D2D1_DRAW_TEXT_OPTIONS_NONE);
                }
                y0 += body.h + 2.0;
                if let (Some(t), Some(b)) = (&refer, brush(soft)) {
                    rt.DrawTextLayout(Vector2 { X: (CARD_W - t.w) / 2.0, Y: y0 }, &t.layout, &b, D2D1_DRAW_TEXT_OPTIONS_NONE);
                    y0 += t.h + 2.0;
                }
                y0 += 8.0;
                // the time it has left
                if let Some(b) = brush(rgba(1.0, 1.0, 1.0, 0.12)) {
                    rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: PADC, top: y0, right: CARD_W - PADC, bottom: y0 + 3.0 }, radiusX: 1.5, radiusY: 1.5 }, &b);
                }
                if let Some(b) = brush(mint) {
                    let r = PADC + (CARD_W - PADC * 2.0) * left;
                    rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: PADC, top: y0, right: r.max(PADC + 3.0), bottom: y0 + 3.0 }, radiusX: 1.5, radiusY: 1.5 }, &b);
                }
                let _ = rt.EndDraw(None, None);
            }
        }
        let pt = POINT { x, y };
        let sz = SIZE { cx: pw, cy: ph };
        let src = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: (alpha * 255.0) as u8, AlphaFormat: AC_SRC_ALPHA as u8 };
        let _ = UpdateLayeredWindow(hwnd, Some(screen), Some(&pt), Some(&sz), Some(mem), Some(&src), COLORREF(0), Some(&blend), ULW_ALPHA);
        SelectObject(mem, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        if !IsWindowVisible(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}
