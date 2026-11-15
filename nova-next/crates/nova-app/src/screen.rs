// Displays in the coordinates nova.js works in (Electron's DIPs): each display's physical origin
// divided by its scale, and points inside a display scaled from that origin. On a single display
// at 100% this is the same as physical pixels.
use windows::Win32::{
    Foundation::{LPARAM, POINT, RECT},
    Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO},
    UI::WindowsAndMessaging::MONITORINFOF_PRIMARY,
    UI::{HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI}, WindowsAndMessaging::GetCursorPos},
};
use windows::core::BOOL;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Display {
    pub bounds: Rect,    // DIPs
    pub work_area: Rect, // DIPs
    pub scale: f64,
    phys: RECT,
    primary: bool,
}

/// One monitor as Windows gives it: its pixels, its work area's pixels, its scale, primary.
type Raw = (RECT, RECT, f64, bool);

unsafe extern "system" fn each(m: HMONITOR, _: HDC, _: *mut RECT, l: LPARAM) -> BOOL {
    let list = &mut *(l.0 as *mut Vec<Raw>);
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    if GetMonitorInfoW(m, &mut mi).as_bool() {
        let (mut dx, mut dy) = (96u32, 96u32);
        let _ = GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        list.push((mi.rcMonitor, mi.rcWork, dx as f64 / 96.0, mi.dwFlags & MONITORINFOF_PRIMARY != 0));
    }
    true.into()
}

pub fn displays() -> Vec<Display> {
    let mut list: Vec<Raw> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(each), LPARAM(&mut list as *mut _ as isize));
    }
    arrange(&list)
}

/// The displays in DIPs, side by side as they are in pixels.
///
/// (1.8.7) Each display's origin was its pixel origin divided by its own scale. With two scales
/// that made displays overlap: a 150% display right of a 1920-pixel one began at 1280, inside
/// the first, so a place saved on it came back on the other display. Now the primary display
/// keeps its origin and every other one is put against the display it touches in pixels (right,
/// left, below or above it), as Electron lays them out; one that touches none keeps the old rule.
/// With one scale for all this is the same as before.
fn arrange(list: &[Raw]) -> Vec<Display> {
    let size = |r: &RECT, s: f64| ((r.right - r.left) as f64 / s, (r.bottom - r.top) as f64 / s);
    let mut origin: Vec<Option<(f64, f64)>> = vec![None; list.len()];
    let first = list.iter().position(|d| d.3).unwrap_or(0);
    if list.is_empty() {
        return Vec::new();
    }
    origin[first] = Some((list[first].0.left as f64 / list[first].2, list[first].0.top as f64 / list[first].2));
    // from the primary outwards: each pass places the displays touching one already placed
    for _ in 0..list.len() {
        let mut moved = false;
        for i in 0..list.len() {
            if origin[i].is_some() {
                continue;
            }
            let (r, s) = (list[i].0, list[i].2);
            let (w, h) = size(&r, s);
            for j in 0..list.len() {
                let Some((px, py)) = origin[j] else { continue };
                let (p, ps) = (list[j].0, list[j].2);
                let (pw, ph) = size(&p, ps);
                let overlap_y = r.top < p.bottom && r.bottom > p.top;
                let overlap_x = r.left < p.right && r.right > p.left;
                let at = if r.left == p.right && overlap_y {
                    Some((px + pw, py + (r.top - p.top) as f64 / ps))
                } else if r.right == p.left && overlap_y {
                    Some((px - w, py + (r.top - p.top) as f64 / ps))
                } else if r.top == p.bottom && overlap_x {
                    Some((px + (r.left - p.left) as f64 / ps, py + ph))
                } else if r.bottom == p.top && overlap_x {
                    Some((px + (r.left - p.left) as f64 / ps, py - h))
                } else {
                    None
                };
                if let Some(o) = at {
                    origin[i] = Some(o);
                    moved = true;
                    break;
                }
            }
        }
        if !moved {
            break;
        }
    }
    list.iter()
        .zip(origin)
        .map(|(&(r, wk, s, primary), o)| {
            let (ox, oy) = o.unwrap_or((r.left as f64 / s, r.top as f64 / s));
            let (w, h) = size(&r, s);
            Display {
                bounds: Rect { x: ox, y: oy, width: w, height: h },
                work_area: Rect { x: ox + (wk.left - r.left) as f64 / s, y: oy + (wk.top - r.top) as f64 / s, width: (wk.right - wk.left) as f64 / s, height: (wk.bottom - wk.top) as f64 / s },
                scale: s,
                phys: r,
                primary,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rc(l: i32, t: i32, r: i32, b: i32) -> RECT {
        RECT { left: l, top: t, right: r, bottom: b }
    }
    fn overlaps(a: &Rect, b: &Rect) -> bool {
        a.x < b.x + b.width - 0.01 && b.x < a.x + a.width - 0.01 && a.y < b.y + b.height - 0.01 && b.y < a.y + a.height - 0.01
    }

    #[test]
    fn one_scale_is_as_before() {
        let d = arrange(&[(rc(0, 0, 1920, 1080), rc(0, 0, 1920, 1040), 1.0, true), (rc(1920, 0, 3840, 1080), rc(1920, 0, 3840, 1040), 1.0, false)]);
        assert_eq!((d[1].bounds.x, d[1].bounds.y, d[1].bounds.width), (1920.0, 0.0, 1920.0));
        let d = arrange(&[(rc(0, 0, 2880, 1620), rc(0, 0, 2880, 1560), 1.5, true), (rc(2880, 0, 5760, 1620), rc(2880, 0, 5760, 1560), 1.5, false)]);
        assert_eq!((d[1].bounds.x, d[1].bounds.width), (1920.0, 1920.0));
    }

    #[test]
    fn two_scales_side_by_side_dont_overlap() {
        // a 1920 x 1080 screen at 100%, a 2880 x 1620 laptop at 150% on its right
        let d = arrange(&[(rc(0, 0, 1920, 1080), rc(0, 0, 1920, 1040), 1.0, true), (rc(1920, 0, 4800, 1620), rc(1920, 0, 4800, 1560), 1.5, false)]);
        assert_eq!(d[1].bounds.x, 1920.0, "it starts where the first ends (it was 1280, inside it)");
        assert!(!overlaps(&d[0].bounds, &d[1].bounds));
        // a place on the second display maps there and back
        let (x, y) = (d[1].bounds.x + 500.0, 300.0);
        let (px, py) = d[1].to_phys(x, y);
        assert_eq!((px, py), (1920 + 750, 450));
        assert!(d[1].contains_phys(px, py) && !d[0].contains_phys(px, py));
        // the laptop on the left (its pixels start below 0), the primary at 125% on the right
        let d = arrange(&[(rc(-2880, 0, 0, 1620), rc(-2880, 0, 0, 1560), 1.5, false), (rc(0, 0, 3200, 1800), rc(0, 0, 3200, 1740), 1.25, true)]);
        assert_eq!((d[0].bounds.x, d[0].bounds.width), (-1920.0, 1920.0));
        assert!(!overlaps(&d[0].bounds, &d[1].bounds));
        // stacked: a 150% display above a 100% primary
        let d = arrange(&[(rc(0, 0, 1920, 1080), rc(0, 0, 1920, 1040), 1.0, true), (rc(0, -1620, 2880, 0), rc(0, -1620, 2880, -60), 1.5, false)]);
        assert_eq!((d[1].bounds.y, d[1].bounds.height), (-1080.0, 1080.0));
        assert!(!overlaps(&d[0].bounds, &d[1].bounds));
        assert_eq!(d[1].work_area.height, 1040.0);
    }
}

pub fn primary() -> Display {
    let all = displays();
    *all.iter().find(|d| d.primary).or(all.first()).expect("a display")
}

fn distance(r: &Rect, x: f64, y: f64) -> f64 {
    let dx = (r.x - x).max(0.0).max(x - (r.x + r.width));
    let dy = (r.y - y).max(0.0).max(y - (r.y + r.height));
    dx.hypot(dy)
}

/// screen.getDisplayNearestPoint (DIPs)
pub fn nearest(x: f64, y: f64) -> Display {
    let all = displays();
    *all.iter().min_by(|a, b| distance(&a.bounds, x, y).partial_cmp(&distance(&b.bounds, x, y)).unwrap()).expect("a display")
}

/// screen.getDisplayMatching: the display a rectangle overlaps most
pub fn matching(r: Rect) -> Display {
    let all = displays();
    let overlap = |d: &Display| {
        let b = d.bounds;
        let w = ((r.x + r.width).min(b.x + b.width) - r.x.max(b.x)).max(0.0);
        let h = ((r.y + r.height).min(b.y + b.height) - r.y.max(b.y)).max(0.0);
        w * h
    };
    let best = all.iter().max_by(|a, b| overlap(a).partial_cmp(&overlap(b)).unwrap()).copied();
    match best {
        Some(d) if overlap(&d) > 0.0 => d,
        _ => nearest(r.x + r.width / 2.0, r.y + r.height / 2.0),
    }
}

impl Display {
    pub fn to_phys(&self, x: f64, y: f64) -> (i32, i32) {
        ((self.phys.left as f64 + (x - self.bounds.x) * self.scale).round() as i32, (self.phys.top as f64 + (y - self.bounds.y) * self.scale).round() as i32)
    }
    fn contains_phys(&self, x: i32, y: i32) -> bool {
        x >= self.phys.left && x < self.phys.right && y >= self.phys.top && y < self.phys.bottom
    }
}

/// A physical point to DIPs (through the display it is on).
pub fn point_to_dip(x: i32, y: i32) -> (f64, f64) {
    let all = displays();
    let d = all.iter().find(|d| d.contains_phys(x, y)).copied().unwrap_or_else(primary);
    (d.bounds.x + (x - d.phys.left) as f64 / d.scale, d.bounds.y + (y - d.phys.top) as f64 / d.scale)
}

/// screen.screenToDipRect
pub fn rect_to_dip(x: i32, y: i32, w: i32, h: i32) -> Rect {
    let all = displays();
    let (cx, cy) = (x + w / 2, y + h / 2);
    let d = all.iter().find(|d| d.contains_phys(cx, cy)).copied().unwrap_or_else(primary);
    let (dx, dy) = (d.bounds.x + (x - d.phys.left) as f64 / d.scale, d.bounds.y + (y - d.phys.top) as f64 / d.scale);
    Rect { x: dx, y: dy, width: w as f64 / d.scale, height: h as f64 / d.scale }
}

/// screen.getCursorScreenPoint (DIPs, whole numbers like Electron's)
pub fn cursor() -> (f64, f64) {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    let (x, y) = point_to_dip(p.x, p.y);
    (x.round(), y.round())
}
