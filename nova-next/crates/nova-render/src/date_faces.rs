//! Date widgets: the Hijri date (4), the day's name (5), the day with both dates (6), the month (7)
//! and the moon (12).
use serde_json::Value;
use windows::{
    core::{Interface, Result},
    Win32::Graphics::Direct2D::{Common::*, *},
};

use crate::{
    clock::{days_in_month, hijri, weekday, Now, DAYS, MONTHS},
    paint::*,
    widget::{Ctx, Face},
};

pub type Cache = Box<dyn Fn() -> Value>;

/// The date changes at midnight: a redraw each minute is enough.
const MINUTE: u32 = 60_000;

/// 4: the Hijri day big, its month in calligraphy, the year.
pub struct HijriDate {
    cache: Cache,
}
impl HijriDate {
    pub fn new(cache: Cache) -> HijriDate {
        HijriDate { cache }
    }
}
impl Face for HijriDate {
    fn size(&self) -> (f32, f32) {
        (176.0, 176.0)
    }
    fn id(&self) -> &str {
        "hijri"
    }
    fn tick(&self) -> u32 {
        MINUTE
    }
    fn draw(&mut self, ctx: &Ctx) -> Result<()> {
        let p = P::new(ctx);
        let (w, h) = self.size();
        p.card(w, h)?;
        let now = Now::get();
        let soft: ID2D1Brush = p.solid(white(0.62))?.cast()?;
        let full: ID2D1Brush = p.solid(white(1.0))?.cast()?;
        let Some((d, month, year)) = hijri(&(self.cache)(), &now) else {
            return p.text("التاريخ الهجري يُحمَّل", ui(13.0, 400), rect(0.0, 0.0, w, h), Align::Center, &soft);
        };
        p.text(DAYS[now.wd as usize], ui(11.5, 400), rect(0.0, 20.0, w, 38.0), Align::Center, &soft)?;
        let num = d.to_string();
        let r = rect(0.0, 38.0, w, 100.0);
        let nw = p.measure(&num, ui(60.0, 700))?;
        p.text(&num, ui(60.0, 700), r, Align::Center, &p.nova(rect(w / 2.0 - nw / 2.0, 46.0, w / 2.0 + nw / 2.0, 96.0))?.cast()?)?;
        p.text(&month, ruqaa(28.0), rect(4.0, 98.0, w - 4.0, 136.0), Align::Center, &full)?;
        p.text(&format!("{year} هـ"), ui(12.0, 400), rect(0.0, 136.0, w, 154.0), Align::Center, &soft)
    }
}

/// 5: the day's name in calligraphy, a short line, the date and the year.
pub struct DayName;
impl Face for DayName {
    fn size(&self) -> (f32, f32) {
        (176.0, 176.0)
    }
    fn id(&self) -> &str {
        "day"
    }
    fn tick(&self) -> u32 {
        MINUTE
    }
    fn draw(&mut self, ctx: &Ctx) -> Result<()> {
        let p = P::new(ctx);
        let (w, h) = self.size();
        p.card(w, h)?;
        let now = Now::get();
        let name = DAYS[now.wd as usize];
        let nw = p.measure(name, ruqaa(52.0))?.min(w - 8.0);
        p.text(name, ruqaa(52.0), rect(4.0, 26.0, w - 4.0, 98.0), Align::Center, &p.nova(rect(w / 2.0 - nw / 2.0, 34.0, w / 2.0 + nw / 2.0, 92.0))?.cast()?)?;
        p.fill_round(rect(w / 2.0 - 18.0, 103.0, w / 2.0 + 18.0, 105.0), 1.0, &p.solid(white(0.25))?.cast()?);
        p.text(&format!("{} {}", now.d, MONTHS[now.mo as usize - 1]), ui(15.0, 600), rect(0.0, 112.0, w, 132.0), Align::Center, &p.solid(white(1.0))?.cast()?)?;
        p.text(&now.y.to_string(), ui(12.0, 400), rect(0.0, 132.0, w, 150.0), Align::Center, &p.solid(white(0.62))?.cast()?)
    }
}

/// 6: the day's name on the right; the Hijri and the Gregorian dates on the left.
pub struct DayDates {
    cache: Cache,
}
impl DayDates {
    pub fn new(cache: Cache) -> DayDates {
        DayDates { cache }
    }
}
impl Face for DayDates {
    fn size(&self) -> (f32, f32) {
        (370.0, 176.0)
    }
    fn id(&self) -> &str {
        "dates"
    }
    fn tick(&self) -> u32 {
        MINUTE
    }
    fn draw(&mut self, ctx: &Ctx) -> Result<()> {
        let p = P::new(ctx);
        let (w, h) = self.size();
        p.card(w, h)?;
        let now = Now::get();
        let soft: ID2D1Brush = p.solid(white(0.62))?.cast()?;
        let full: ID2D1Brush = p.solid(white(1.0))?.cast()?;

        // the name, in the right part
        let split = 176.0;
        let name = DAYS[now.wd as usize];
        let mid = (split + w) / 2.0;
        let nw = p.measure(name, ruqaa(54.0))?.min(w - split - 8.0);
        p.text(name, ruqaa(54.0), rect(split + 4.0, 0.0, w - 4.0, h), Align::Center, &p.nova(rect(mid - nw / 2.0, 58.0, mid + nw / 2.0, 118.0))?.cast()?)?;
        p.fill_round(rect(split, 26.0, split + 1.0, h - 26.0), 0.0, &p.solid(white(0.12))?.cast()?);

        // the dates, right-aligned in the left part
        let (l, r) = (22.0, split - 22.0);
        p.text("هجري", ui(11.0, 400), rect(l, 30.0, r, 46.0), Align::Right, &soft)?;
        match hijri(&(self.cache)(), &now) {
            Some((d, month, year)) => {
                let num = d.to_string();
                let dw = p.measure(&num, ui(30.0, 700))?;
                p.text(&num, ui(30.0, 700), rect(l, 44.0, r, 84.0), Align::Right, &full)?;
                p.text(&month, ruqaa(20.0), rect(l - 20.0, 46.0, r - dw - 8.0, 84.0), Align::Right, &full)?;
                p.text(&format!("{year} هـ"), ui(11.0, 400), rect(l, 82.0, r, 98.0), Align::Right, &soft)?;
            }
            None => p.text("—", ui(20.0, 600), rect(l, 44.0, r, 84.0), Align::Right, &soft)?,
        }
        p.text("ميلادي", ui(11.0, 400), rect(l, 108.0, r, 124.0), Align::Right, &soft)?;
        p.text(&format!("{} {} {}", now.d, MONTHS[now.mo as usize - 1], now.y), ui(16.0, 600), rect(l - 16.0, 124.0, r, 146.0), Align::Right, &full)
    }
}

/// 7: this month, the week from Saturday (on the right), today in a Nova circle, Fridays in violet.
pub struct Month {
    cache: Cache,
}
impl Month {
    pub fn new(cache: Cache) -> Month {
        Month { cache }
    }
}
impl Face for Month {
    fn size(&self) -> (f32, f32) {
        (176.0, 228.0)
    }
    fn id(&self) -> &str {
        "month"
    }
    fn tick(&self) -> u32 {
        MINUTE
    }
    fn draw(&mut self, ctx: &Ctx) -> Result<()> {
        let p = P::new(ctx);
        let (w, h) = self.size();
        p.card(w, h)?;
        let now = Now::get();
        let (l, r) = (12.0, w - 12.0);
        p.text(MONTHS[now.mo as usize - 1], ruqaa(22.0), rect(l, 10.0, r, 42.0), Align::Right, &p.solid(white(1.0))?.cast()?)?;
        if let Some((_, month, _)) = hijri(&(self.cache)(), &now) {
            p.text(&month, ui(11.0, 400), rect(l, 16.0, l + 80.0, 38.0), Align::Left, &p.solid(white(0.62))?.cast()?)?;
        }
        let cw = (r - l) / 7.0;
        let col_x = |c: usize| r - (c as f32 + 1.0) * cw;
        let head: ID2D1Brush = p.solid(white(0.45))?.cast()?;
        for (c, d) in ["س", "ح", "ن", "ث", "ر", "خ", "ج"].iter().enumerate() {
            p.text(d, ui(11.0, 400), rect(col_x(c), 48.0, col_x(c) + cw, 66.0), Align::Center, &head)?;
        }
        // Saturday is column 0
        let first = ((weekday(now.y, now.mo, 1) + 1) % 7) as usize;
        let (py, pm) = if now.mo == 1 { (now.y - 1, 12) } else { (now.y, now.mo - 1) };
        let prev_days = days_in_month(py, pm);
        let dim: ID2D1Brush = p.solid(white(0.25))?.cast()?;
        let day: ID2D1Brush = p.solid(white(0.85))?.cast()?;
        let fri: ID2D1Brush = p.solid(rgba(201, 184, 255, 1.0))?.cast()?;
        let full: ID2D1Brush = p.solid(white(1.0))?.cast()?;
        let row_y = |i: usize| 68.0 + (i / 7) as f32 * 23.0;
        for i in 0..first {
            let n = prev_days as usize - first + 1 + i;
            p.text(&n.to_string(), ui(11.0, 400), rect(col_x(i), row_y(i), col_x(i) + cw, row_y(i) + 22.0), Align::Center, &dim)?;
        }
        for d in 1..=days_in_month(now.y, now.mo) as usize {
            let i = first + d - 1;
            let c = i % 7;
            let (x0, y0) = (col_x(c), row_y(i));
            let bx = rect(x0, y0, x0 + cw, y0 + 22.0);
            if d as u32 == now.d {
                let (cx, cy) = (x0 + cw / 2.0, y0 + 11.0);
                p.fill_circle(cx, cy, 10.0, &p.nova(rect(cx - 10.0, cy - 10.0, cx + 10.0, cy + 10.0))?.cast()?);
                p.text(&d.to_string(), ui(11.0, 700), bx, Align::Center, &full)?;
            } else {
                p.text(&d.to_string(), ui(11.0, 400), bx, Align::Center, if c == 6 { &fri } else { &day })?;
            }
        }
        Ok(())
    }
}

/// 12: the moon as it is tonight (from the Hijri day), its phase's name and how much is lit.
pub struct Moon {
    cache: Cache,
}
impl Moon {
    pub fn new(cache: Cache) -> Moon {
        Moon { cache }
    }
}

/// The phase's Arabic name for a Hijri day.
pub fn phase_name(day: u32) -> &'static str {
    match day {
        1..=6 => "هلال أول الشهر",
        7..=8 => "التربيع الأول",
        9..=13 => "أحدب متزايد",
        14..=16 => "البدر",
        17..=21 => "أحدب متناقص",
        22..=23 => "التربيع الأخير",
        _ => "هلال آخر الشهر",
    }
}
/// The moon's age in days for a Hijri day (the month starts with the new crescent).
fn age(day: u32) -> f32 {
    day as f32 - 0.5
}
/// The share of the disc that is lit, 0..1.
pub fn lit(day: u32) -> f32 {
    let th = 2.0 * std::f32::consts::PI * age(day) / 29.53;
    (1.0 - th.cos()) / 2.0
}

impl Face for Moon {
    fn size(&self) -> (f32, f32) {
        (176.0, 176.0)
    }
    fn id(&self) -> &str {
        "moon"
    }
    fn tick(&self) -> u32 {
        MINUTE
    }
    fn draw(&mut self, ctx: &Ctx) -> Result<()> {
        let p = P::new(ctx);
        let (w, h) = self.size();
        p.card(w, h)?;
        let now = Now::get();
        let soft: ID2D1Brush = p.solid(white(0.62))?.cast()?;
        let Some((d, month, _)) = hijri(&(self.cache)(), &now) else {
            return p.text("التاريخ الهجري يُحمَّل", ui(13.0, 400), rect(0.0, 0.0, w, h), Align::Center, &soft);
        };
        let (cx, cy, r) = (88.0, 66.0, 34.0);
        p.stroke_circle(cx, cy, 40.0, &p.solid(rgba(255, 240, 200, 0.12))?.cast()?, 6.0);
        p.fill_circle(cx, cy, r, &p.solid(white(0.07))?.cast()?);

        // the lit part: the bright limb's half circle, closed by the terminator (a half ellipse)
        let th = 2.0 * std::f32::consts::PI * age(d) / 29.53;
        // the first and last nights' crescent is a hair: kept thick enough to be seen
        let k = (r * th.cos().abs()).min(r * 0.82);
        let waxing = th < std::f32::consts::PI;
        let crescent = th.cos() > 0.0;
        let g = p.geometry()?;
        unsafe {
            let s = g.Open()?;
            let top = pt(cx, cy - r);
            let bottom = pt(cx, cy + r);
            s.BeginFigure(top, D2D1_FIGURE_BEGIN_FILLED);
            // waxing: lit on the right (top → right → bottom, clockwise); waning: on the left
            s.AddArc(&D2D1_ARC_SEGMENT {
                point: bottom,
                size: D2D_SIZE_F { width: r, height: r },
                rotationAngle: 0.0,
                sweepDirection: if waxing { D2D1_SWEEP_DIRECTION_CLOCKWISE } else { D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE },
                arcSize: D2D1_ARC_SIZE_SMALL,
            });
            // back to the top through the terminator: towards the lit side for a crescent
            let towards_right = waxing == crescent;
            s.AddArc(&D2D1_ARC_SEGMENT {
                point: top,
                size: D2D_SIZE_F { width: k.max(0.01), height: r },
                rotationAngle: 0.0,
                // bottom → right → top runs counter-clockwise on screen
                sweepDirection: if towards_right { D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE } else { D2D1_SWEEP_DIRECTION_CLOCKWISE },
                arcSize: D2D1_ARC_SIZE_SMALL,
            });
            s.EndFigure(D2D1_FIGURE_END_CLOSED);
            s.Close()?;
        }
        let light = p.radial((cx - 0.3 * r, cy - 0.3 * r), 1.6 * r, 1.6 * r, &[(0.0, rgba(255, 248, 225, 1.0)), (1.0, rgba(217, 199, 154, 1.0))])?;
        p.fill_geo(&g, &light.cast()?);

        p.text(phase_name(d), ruqaa(20.0), rect(4.0, 110.0, w - 4.0, 142.0), Align::Center, &p.solid(white(1.0))?.cast()?)?;
        p.text(&format!("{d} {month} · نور {}%", (lit(d) * 100.0).round() as i32), ui(11.0, 400), rect(4.0, 142.0, w - 4.0, 160.0), Align::Center, &soft)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn moon_light() {
        assert!(lit(1) < 0.02);
        assert!(lit(15) > 0.97);
        assert!(lit(29) < 0.05);
        assert_eq!(phase_name(29), "هلال آخر الشهر");
    }
}
