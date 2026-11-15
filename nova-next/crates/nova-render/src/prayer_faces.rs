//! Prayer widgets: the five prayers in a row (2) and the sun's path through the day (3).
use serde_json::Value;
use windows::{core::{Interface, Result}, Win32::Graphics::Direct2D::ID2D1Brush};

use crate::{
    clock::{minutes, schedule, show, show_short, time_left_ar, cache_day, Now},
    paint::*,
    widget::{Ctx, Face},
};

pub type Cache = Box<dyn Fn() -> Value>;

fn waiting(p: &P, w: f32, h: f32) -> Result<()> {
    p.text("مواقيت الصلاة تُحمَّل", ui(13.0, 400), rect(0.0, 0.0, w, h), Align::Center, &p.solid(white(0.7))?.cast::<ID2D1Brush>()?)
}

/// 2: the next prayer and the time left, a bar through the day's prayers, the five times.
pub struct PrayerRow {
    cache: Cache,
}
impl PrayerRow {
    pub fn new(cache: Cache) -> PrayerRow {
        PrayerRow { cache }
    }
}

impl Face for PrayerRow {
    fn size(&self) -> (f32, f32) {
        (370.0, 176.0)
    }
    fn id(&self) -> &str {
        "prayers"
    }
    fn tick(&self) -> u32 {
        15_000
    }
    fn draw(&mut self, ctx: &Ctx) -> Result<()> {
        let p = P::new(ctx);
        let (w, h) = self.size();
        p.card(w, h)?;
        let c = (self.cache)();
        // (1.8.7) the place's clock, not the PC's
        let now = Now::get().at_place(&c);
        let Some(s) = schedule(&c, &now) else { return waiting(&p, w, h) };
        let (l, r) = (18.0, w - 18.0);
        let full: ID2D1Brush = p.solid(white(1.0))?.cast()?;
        let soft: ID2D1Brush = p.solid(white(0.62))?.cast()?;

        // the next prayer, the time left beside it, the city on the other side
        let next = &s.list[s.next];
        let f_name = ruqaa(26.0);
        let nw = p.measure(&next.name, f_name)?;
        p.text(&next.name, f_name, rect(l, 12.0, r, 52.0), Align::Right, &full)?;
        p.text(&time_left_ar(s.left), ui(12.0, 400), rect(l, 18.0, r - nw - 10.0, 50.0), Align::Right, &soft)?;
        if let Some(city) = c["city"].as_str() {
            p.text(city, ui(12.0, 400), rect(l, 18.0, l + 120.0, 50.0), Align::Left, &soft)?;
        }

        // the bar: how far the day is from the fajr to the isha
        let first = s.list[0].min as f32;
        let last = s.list[4].min as f32;
        let share = if s.tomorrow { 0.0 } else { ((now.minutes() as f32 - first) / (last - first).max(1.0)).clamp(0.0, 1.0) };
        let (by, bw) = (66.0, r - l);
        p.fill_round(rect(l, by, r, by + 4.0), 2.0, &p.solid(white(0.12))?.cast()?);
        if share > 0.0 {
            let x = r - bw * share;
            p.fill_round(rect(x, by, r, by + 4.0), 2.0, &p.linear((r, 0.0), (l, 0.0), c3(BLUE, 1.0), c3(VIOLET, 1.0))?.cast()?);
        }
        p.fill_circle(r - bw * share, by + 2.0, 6.0, &full);

        // the five prayers, the fajr on the right; the next one in a violet box
        let gap = 6.0;
        let cw = (bw - gap * 4.0) / 5.0;
        for (i, pr) in s.list.iter().enumerate() {
            let x1 = r - i as f32 * (cw + gap);
            let x0 = x1 - cw;
            let on = i == s.next;
            if on {
                let bx = rect(x0, 86.0, x1, 140.0);
                p.fill_round(bx, 12.0, &p.solid(rgba(143, 123, 255, 0.28))?.cast()?);
                p.stroke_round(rect(x0 + 0.5, 86.5, x1 - 0.5, 139.5), 12.0, &p.solid(rgba(178, 107, 255, 0.5))?.cast()?, 1.0);
            }
            let b = if on { &full } else { &soft };
            p.text(&pr.name, ui(12.0, 400), rect(x0, 92.0, x1, 110.0), Align::Center, b)?;
            // the time as set; without ص / م when the box is too narrow for it
            let at = show(&pr.at);
            let at = if p.measure(&at, ui(14.0, 600))? > cw - 4.0 { show_short(&pr.at) } else { at };
            p.text(&at, ui(14.0, 600), rect(x0, 110.0, x1, 132.0), Align::Center, b)?;
        }
        Ok(())
    }
}

/// 3: the sun on an arc from sunrise (right) to sunset (left); the next prayer under it.
pub struct SunPath {
    cache: Cache,
}
impl SunPath {
    pub fn new(cache: Cache) -> SunPath {
        SunPath { cache }
    }
}

impl Face for SunPath {
    fn size(&self) -> (f32, f32) {
        (370.0, 176.0)
    }
    fn id(&self) -> &str {
        "sun"
    }
    fn tick(&self) -> u32 {
        30_000
    }
    fn draw(&mut self, ctx: &Ctx) -> Result<()> {
        let p = P::new(ctx);
        let (w, h) = self.size();
        p.card(w, h)?;
        let c = (self.cache)();
        // (1.8.7) the place's clock, not the PC's
        let now = Now::get().at_place(&c);
        let Some(s) = schedule(&c, &now) else { return waiting(&p, w, h) };
        let day = cache_day(&c, &now).cloned().unwrap_or_default();
        let rise_s = day["times"]["sunrise"].as_str().unwrap_or("06:00").to_string();
        let set_s = day["times"]["maghrib"].as_str().unwrap_or("18:00").to_string();
        let (rise, set) = (minutes(&rise_s).unwrap_or(360) as f32, minutes(&set_s).unwrap_or(1080) as f32);
        let soft: ID2D1Brush = p.solid(white(0.62))?.cast()?;
        let faint: ID2D1Brush = p.solid(white(0.14))?.cast()?;

        // the horizon, and the whole path dotted
        let (cx, cy, rx, ry) = (185.0, 132.0, 140.0, 104.0);
        p.line((20.0, cy), (350.0, cy), &faint, 1.0, false)?;
        p.arc_deg(cx, cy, rx, ry, 180.0, 360.0, &faint, 3.0, Some(&[2.0 / 3.0, 2.0]))?;

        // the sun's way so far (sunrise is on the right: angle 360, sunset 180)
        let t = (now.minutes() as f32 - rise) / (set - rise).max(1.0);
        if (0.0..=1.0).contains(&t) {
            let a = 360.0 - 180.0 * t;
            let (sx, sy) = (cx + rx * a.to_radians().cos(), cy + ry * a.to_radians().sin());
            if t > 0.005 {
                let g = p.linear((sx, 0.0), (cx + rx, 0.0), rgba(255, 179, 92, 1.0), c3(BLUE, 1.0))?;
                p.arc_deg(cx, cy, rx, ry, 360.0, a, &g.cast()?, 3.0, None)?;
            }
            p.fill_circle(sx, sy, 14.0, &p.solid(rgba(255, 196, 107, 0.25))?.cast()?);
            p.fill_circle(sx, sy, 8.0, &p.solid(rgba(255, 210, 127, 1.0))?.cast()?);
        }

        p.text(&format!("الشروق {}", show(&rise_s)), ui(11.0, 400), rect(cx + rx - 60.0, 142.0, cx + rx + 60.0, 162.0), Align::Center, &soft)?;
        p.text(&format!("الغروب {}", show(&set_s)), ui(11.0, 400), rect(cx - rx - 60.0, 142.0, cx - rx + 60.0, 162.0), Align::Center, &soft)?;

        let next = &s.list[s.next];
        p.text(&format!("{} {}", next.name, show(&next.at)), ruqaa(24.0), rect(0.0, 60.0, w, 96.0), Align::Center, &p.solid(white(1.0))?.cast()?)?;
        p.text(&time_left_ar(s.left), ui(11.5, 400), rect(0.0, 96.0, w, 114.0), Align::Center, &soft)?;
        Ok(())
    }
}
