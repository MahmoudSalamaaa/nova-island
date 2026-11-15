//! Clipboard text (Electron's clipboard.readText / writeText).
use windows::Win32::{
    Foundation::{HANDLE, HGLOBAL},
    System::{
        DataExchange::{CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData},
        Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
    },
};

const CF_UNICODETEXT: u32 = 13;
/// Longest clipboard text read, in UTF-16 units (8 MB).
const MAX_TEXT: usize = 4 << 20;
/// Largest clipboard picture read, in pixels (an 8K screenshot is 33 M).
const MAX_PIXELS: u64 = 40_000_000;

/// Opens the clipboard, retrying briefly while another app holds it.
fn open() -> bool {
    for _ in 0..10 {
        if unsafe { OpenClipboard(None) }.is_ok() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    false
}

/// The text on the clipboard ("" when there is none).
pub fn read_text() -> String {
    if !open() {
        return String::new();
    }
    let text = unsafe {
        match GetClipboardData(CF_UNICODETEXT) {
            Ok(h) if !h.is_invalid() => {
                let g = HGLOBAL(h.0);
                let p = GlobalLock(g) as *const u16;
                let s = if p.is_null() {
                    String::new()
                } else {
                    // (1.8.7) another app's data: read only inside its block (it may have no NUL)
                    // and at most MAX_TEXT units
                    let units = (windows::Win32::System::Memory::GlobalSize(g) / 2).min(MAX_TEXT);
                    let all = std::slice::from_raw_parts(p, units);
                    let n = all.iter().position(|&c| c == 0).unwrap_or(units);
                    String::from_utf16_lossy(&all[..n])
                };
                let _ = GlobalUnlock(g);
                s
            }
            _ => String::new(),
        }
    };
    unsafe {
        let _ = CloseClipboard();
    }
    text
}

pub fn write_text(text: &str) -> bool {
    let w: Vec<u16> = text.encode_utf16().chain([0]).collect();
    if !open() {
        return false;
    }
    let ok = unsafe {
        let _ = EmptyClipboard();
        match GlobalAlloc(GMEM_MOVEABLE, w.len() * 2) {
            Ok(g) => {
                let p = GlobalLock(g) as *mut u16;
                if !p.is_null() {
                    std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
                }
                let _ = GlobalUnlock(g);
                SetClipboardData(CF_UNICODETEXT, Some(HANDLE(g.0))).is_ok()
            }
            Err(_) => false,
        }
    };
    unsafe {
        let _ = CloseClipboard();
    }
    ok
}

/// The image on the clipboard as top-down BGRA (w, h, pixels), or None when there is none.
pub fn read_image() -> Option<(u32, u32, Vec<u8>)> {
    const CF_DIB: u32 = 8;
    if !open() {
        return None;
    }
    let out = unsafe {
        (|| {
            let h = GetClipboardData(CF_DIB).ok().filter(|h| !h.is_invalid())?;
            let g = HGLOBAL(h.0);
            let p = GlobalLock(g) as *const u8;
            if p.is_null() {
                return None;
            }
            let size = windows::Win32::System::Memory::GlobalSize(g);
            let data = std::slice::from_raw_parts(p, size);
            let r = (|| {
                let hdr = u32::from_le_bytes(data.get(0..4)?.try_into().ok()?) as usize;
                let w = i32::from_le_bytes(data.get(4..8)?.try_into().ok()?);
                let hh = i32::from_le_bytes(data.get(8..12)?.try_into().ok()?);
                let bpp = u16::from_le_bytes(data.get(14..16)?.try_into().ok()?);
                let comp = u32::from_le_bytes(data.get(16..20)?.try_into().ok()?);
                let colors = u32::from_le_bytes(data.get(32..36)?.try_into().ok()?) as usize;
                if w <= 0 || hh == 0 || !(bpp == 24 || bpp == 32) {
                    return None;
                }
                let (w, top_down, h) = (w as u32, hh < 0, hh.unsigned_abs());
                // BI_BITFIELDS keeps 3 masks after a 40-byte header
                let masks = if comp == 3 && hdr == 40 { 12 } else { 0 };
                let off = hdr + masks + colors * 4;
                // (1.8.7) sizes from another app: checked, and every row there before allocating
                if w as u64 * h as u64 > MAX_PIXELS {
                    return None;
                }
                let stride = ((w as usize * bpp as usize / 8) + 3) & !3;
                if off.checked_add(stride.checked_mul(h as usize)?)? > data.len() {
                    return None;
                }
                let mut px = vec![0u8; w as usize * h as usize * 4];
                let mut any_alpha = false;
                for y in 0..h as usize {
                    let sy = if top_down { y } else { h as usize - 1 - y };
                    let row = data.get(off + sy * stride..off + sy * stride + stride)?;
                    for x in 0..w as usize {
                        let o = (y * w as usize + x) * 4;
                        if bpp == 32 {
                            px[o..o + 4].copy_from_slice(&row[x * 4..x * 4 + 4]);
                            any_alpha |= row[x * 4 + 3] != 0;
                        } else {
                            px[o..o + 3].copy_from_slice(&row[x * 3..x * 3 + 3]);
                        }
                    }
                }
                // most apps leave alpha at 0 in a 32-bit DIB: then the picture is opaque
                if !any_alpha {
                    for a in px.iter_mut().skip(3).step_by(4) {
                        *a = 255;
                    }
                }
                Some((w, h, px))
            })();
            let _ = GlobalUnlock(g);
            r
        })()
    };
    unsafe {
        let _ = CloseClipboard();
    }
    out
}
