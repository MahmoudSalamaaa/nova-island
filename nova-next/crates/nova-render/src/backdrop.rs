//! The widgets' frosted glass, drawn by the widget itself: the wallpaper of the widget's screen,
//! placed as Windows places it (fill, fit, stretch, centre, tile, span), made small, blurred and
//! a little more saturated, kept once per screen. Each widget fills its rounded card with the part
//! of it under the window, so the corners are smooth and outside them the window is clear.
//! (Windows' own acrylic blurs the whole square window, whatever its shape.)
use std::cell::RefCell;

use windows::{
    core::{Interface, Result, PCWSTR},
    Win32::{
        Foundation::{GENERIC_READ, HWND, RECT},
        Graphics::{
            Direct2D::{Common::*, *},
            Dxgi::Common::*,
            Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST},
            Imaging::*,
        },
        System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_APARTMENTTHREADED},
        UI::{Shell::*, WindowsAndMessaging::GetWindowRect},
    },
};
use windows_numerics::Matrix3x2;

/// The blurred copy is this many times smaller than the screen.
const SHRINK: u32 = 4;
/// Box blur radius (small pixels) and passes: three passes are close to a Gaussian.
const BLUR: usize = 6;
const PASSES: usize = 3;
const SATURATE: f32 = 1.5;

struct Glass {
    /// the screen, physical pixels
    mon: RECT,
    w: u32,
    h: u32,
    /// premultiplied BGRA
    px: Vec<u8>,
}

thread_local! {
    static CACHE: RefCell<Vec<Glass>> = const { RefCell::new(Vec::new()) };
    static MAKING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Makes the glass for the window's screen if there's none yet. Called before a widget paints,
/// with nothing borrowed: loading the picture goes through COM, which can deliver other
/// widgets' paint messages meanwhile.
/// True when it made a new one (the widgets that painted without it can paint again).
pub fn prepare(hwnd: HWND) -> bool {
    let Some(mon) = monitor_of(hwnd) else { return false };
    if CACHE.with(|c| c.try_borrow().map(|c| c.iter().any(|g| g.mon == mon)).unwrap_or(true)) || MAKING.with(|m| m.get()) {
        return false;
    }
    MAKING.with(|m| m.set(true));
    let g = make(mon);
    MAKING.with(|m| m.set(false));
    let Some(g) = g else { return false };
    CACHE.with(|c| {
        c.try_borrow_mut()
            .map(|mut c| {
                c.retain(|x| x.mon != mon);
                c.push(g);
            })
            .is_ok()
    })
}

/// The wallpaper changed (or the screens did): made again on the next paint.
pub fn forget() {
    CACHE.with(|c| c.borrow_mut().clear());
}

fn monitor_of(hwnd: HWND) -> Option<RECT> {
    unsafe {
        let m = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        GetMonitorInfoW(m, &mut mi).as_bool().then_some(mi.rcMonitor)
    }
}

/// The wallpaper file for a screen and how it's placed; the background colour.
fn wallpaper(mon: &RECT) -> Option<(String, DESKTOP_WALLPAPER_POSITION, u32)> {
    unsafe {
        let dw: IDesktopWallpaper = CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL).ok()?;
        let pos = dw.GetPosition().unwrap_or(DWPOS_FILL);
        let color = dw.GetBackgroundColor().map(|c| c.0).unwrap_or(0);
        let n = dw.GetMonitorDevicePathCount().unwrap_or(0);
        let mut path = String::new();
        for i in 0..n {
            let Ok(id) = dw.GetMonitorDevicePathAt(i) else { continue };
            let r = dw.GetMonitorRECT(PCWSTR(id.0));
            if r.map(|r| r == *mon).unwrap_or(false) || n == 1 {
                if let Ok(p) = dw.GetWallpaper(PCWSTR(id.0)) {
                    path = p.to_string().unwrap_or_default();
                    CoTaskMemFree(Some(p.0 as _));
                }
            }
            CoTaskMemFree(Some(id.0 as _));
            if !path.is_empty() {
                break;
            }
        }
        // a slideshow or Spotlight: the picture Windows is showing now
        if path.is_empty() || !std::path::Path::new(&path).exists() {
            let t = std::path::PathBuf::from(std::env::var("APPDATA").ok()?).join("Microsoft\\Windows\\Themes\\TranscodedWallpaper");
            if t.exists() {
                path = t.to_string_lossy().into_owned();
            }
        }
        Some((path, pos, color))
    }
}

/// The picture at (w, h), premultiplied BGRA.
fn decode(path: &str, w: u32, h: u32) -> Result<Vec<u8>> {
    unsafe {
        let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_ALL)?;
        let dec = wic.CreateDecoderFromFilename(&windows::core::HSTRING::from(path), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand)?;
        let frame = dec.GetFrame(0)?;
        let scaler = wic.CreateBitmapScaler()?;
        scaler.Initialize(&frame, w.max(1), h.max(1), WICBitmapInterpolationModeFant)?;
        let conv = wic.CreateFormatConverter()?;
        conv.Initialize(&scaler, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)?;
        let mut buf = vec![0u8; (w.max(1) * h.max(1) * 4) as usize];
        conv.CopyPixels(std::ptr::null(), w.max(1) * 4, &mut buf)?;
        Ok(buf)
    }
}

fn size_of(path: &str) -> Option<(u32, u32)> {
    unsafe {
        let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_ALL).ok()?;
        let dec = wic.CreateDecoderFromFilename(&windows::core::HSTRING::from(path), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand).ok()?;
        let (mut w, mut h) = (0, 0);
        dec.GetFrame(0).ok()?.GetSize(&mut w, &mut h).ok()?;
        Some((w, h))
    }
}

/// A box blur along rows (`horizontal`) or columns, in place.
fn box_blur(px: &mut [u8], w: usize, h: usize, horizontal: bool) {
    let (n, lines) = if horizontal { (w, h) } else { (h, w) };
    let idx = |line: usize, i: usize| if horizontal { (line * w + i) * 4 } else { (i * w + line) * 4 };
    let mut row = vec![[0u32; 4]; n];
    for line in 0..lines {
        for (i, r) in row.iter_mut().enumerate() {
            let k = idx(line, i);
            *r = [px[k] as u32, px[k + 1] as u32, px[k + 2] as u32, px[k + 3] as u32];
        }
        let mut sum = [0u32; 4];
        let at = |i: isize| row[i.clamp(0, n as isize - 1) as usize];
        for i in -(BLUR as isize)..=(BLUR as isize) {
            let v = at(i);
            (0..4).for_each(|c| sum[c] += v[c]);
        }
        let div = (2 * BLUR + 1) as u32;
        for i in 0..n {
            let k = idx(line, i);
            (0..4).for_each(|c| px[k + c] = (sum[c] / div) as u8);
            let (add, sub) = (at(i as isize + BLUR as isize + 1), at(i as isize - BLUR as isize));
            (0..4).for_each(|c| sum[c] = sum[c] + add[c] - sub[c]);
        }
    }
}

/// The screen's wallpaper, small, placed, blurred.
fn make(mon: RECT) -> Option<Glass> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let (mw, mh) = ((mon.right - mon.left) as u32, (mon.bottom - mon.top) as u32);
    let (w, h) = ((mw / SHRINK).max(1), (mh / SHRINK).max(1));
    let (path, pos, color) = wallpaper(&mon)?;
    // the background colour first (seen around a fitted or centred picture, or alone)
    let (r, g, b) = ((color & 0xff) as u8, ((color >> 8) & 0xff) as u8, ((color >> 16) & 0xff) as u8);
    let mut px: Vec<u8> = std::iter::repeat_n([b, g, r, 255], (w * h) as usize).flatten().collect();
    if let Some((iw, ih)) = (!path.is_empty()).then(|| size_of(&path)).flatten() {
        let (sx, sy) = (mw as f32 / iw as f32, mh as f32 / ih as f32);
        let (dw, dh) = match pos {
            p if p == DWPOS_STRETCH => (mw as f32, mh as f32),
            p if p == DWPOS_FIT => {
                let s = sx.min(sy);
                (iw as f32 * s, ih as f32 * s)
            }
            p if p == DWPOS_CENTER => (iw as f32, ih as f32),
            // fill, span and tile: the picture covers the screen
            _ => {
                let s = sx.max(sy);
                (iw as f32 * s, ih as f32 * s)
            }
        };
        let (pw, ph) = (((dw / SHRINK as f32).round() as u32).max(1), ((dh / SHRINK as f32).round() as u32).max(1));
        if let Ok(pic) = decode(&path, pw, ph) {
            // centred on the screen, cut to it
            let (ox, oy) = ((w as i64 - pw as i64) / 2, (h as i64 - ph as i64) / 2);
            for y in 0..h as i64 {
                let py = y - oy;
                if py < 0 || py >= ph as i64 {
                    continue;
                }
                for x in 0..w as i64 {
                    let pxx = x - ox;
                    if pxx < 0 || pxx >= pw as i64 {
                        continue;
                    }
                    let (d, s) = (((y * w as i64 + x) * 4) as usize, ((py * pw as i64 + pxx) * 4) as usize);
                    px[d..d + 4].copy_from_slice(&pic[s..s + 4]);
                }
            }
        }
    }
    for _ in 0..PASSES {
        box_blur(&mut px, w as usize, h as usize, true);
        box_blur(&mut px, w as usize, h as usize, false);
    }
    // a little more colour, as the design's glass (CSS saturate 1.5)
    for p in px.chunks_exact_mut(4) {
        let l = 0.114 * p[0] as f32 + 0.587 * p[1] as f32 + 0.299 * p[2] as f32;
        for c in p.iter_mut().take(3) {
            *c = (l + (*c as f32 - l) * SATURATE).clamp(0.0, 255.0) as u8;
        }
        p[3] = 255;
    }
    Some(Glass { mon, w, h, px })
}

/// Fills the card (`rr`, DIPs) with the blurred wallpaper under the window; false if there's none.
pub fn fill(rt: &ID2D1HwndRenderTarget, hwnd: HWND, scale: f32, rr: &D2D1_ROUNDED_RECT) -> Result<bool> {
    let Some(mon) = monitor_of(hwnd) else { return Ok(false) };
    let mut win = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut win)? };
    CACHE.with(|c| -> Result<bool> {
        // made by prepare(); none (yet): the solid glass this time
        let Ok(c) = c.try_borrow() else { return Ok(false) };
        let Some(g) = c.iter().find(|g| g.mon == mon) else { return Ok(false) };
        // the small pixels under the window, with a pixel's margin for smooth edges
        let k = SHRINK as i32;
        let x0 = ((win.left - mon.left) / k - 1).clamp(0, g.w as i32 - 1);
        let y0 = ((win.top - mon.top) / k - 1).clamp(0, g.h as i32 - 1);
        let x1 = ((win.right - mon.left) / k + 2).clamp(x0 + 1, g.w as i32);
        let y1 = ((win.bottom - mon.top) / k + 2).clamp(y0 + 1, g.h as i32);
        let (cw, ch) = ((x1 - x0) as usize, (y1 - y0) as usize);
        let mut crop = Vec::with_capacity(cw * ch * 4);
        for y in y0..y1 {
            let s = ((y as u32 * g.w + x0 as u32) * 4) as usize;
            crop.extend_from_slice(&g.px[s..s + cw * 4]);
        }
        unsafe {
            let props = D2D1_BITMAP_PROPERTIES { pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED }, dpiX: 96.0, dpiY: 96.0 };
            let bmp = rt.CreateBitmap(D2D_SIZE_U { width: cw as u32, height: ch as u32 }, Some(crop.as_ptr() as _), (cw * 4) as u32, &props)?;
            let bp = D2D1_BITMAP_BRUSH_PROPERTIES { extendModeX: D2D1_EXTEND_MODE_CLAMP, extendModeY: D2D1_EXTEND_MODE_CLAMP, interpolationMode: D2D1_BITMAP_INTERPOLATION_MODE_LINEAR };
            let brush = rt.CreateBitmapBrush(&bmp, Some(&bp), None)?;
            // a small pixel is SHRINK screen pixels; the window's DIPs are screen pixels / scale
            let m = k as f32 / scale;
            brush.SetTransform(&Matrix3x2 {
                M11: m,
                M12: 0.0,
                M21: 0.0,
                M22: m,
                M31: (mon.left + x0 * k - win.left) as f32 / scale,
                M32: (mon.top + y0 * k - win.top) as f32 / scale,
            });
            rt.FillRoundedRectangle(rr, &brush.cast::<ID2D1Brush>()?);
        }
        Ok(true)
    })
}
