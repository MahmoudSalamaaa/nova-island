//! The widgets' faces. First: the next prayer, a ring of the time between the last prayer and the
//! next, its time, its name in calligraphy and the time left (from prayer-cache.json, the same
//! times the island shows).
use serde_json::Value;
use windows::{
    core::{Result, HSTRING},
    Win32::{
        Foundation::SYSTEMTIME,
        Graphics::{
            Direct2D::{Common::*, *},
            DirectWrite::*,
        },
        System::SystemInformation::GetLocalTime,
    },
};

use windows_numerics::Vector2;

use crate::{
    font::{CALLIGRAPHY, UI},
    widget::{Ctx, Face},
};

const PRAYERS: [(&str, &str); 5] = [("fajr", "الفجر"), ("dhuhr", "الظهر"), ("asr", "العصر"), ("maghrib", "المغرب"), ("isha", "العشاء")];

fn rgba(r: u8, g: u8, b: u8, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a }
}

fn minutes(hhmm: &str) -> Option<i32> {
    let (h, m) = hhmm.split_once(':')?;
    Some(h.trim().parse::<i32>().ok()? * 60 + m.trim().parse::<i32>().ok()?)
}

/// "بعد 45 دقيقة", "بعد ساعتين و 5 دقائق" (Arabic counts, Western digits as in Settings)
pub fn time_left_ar(mins: i32) -> String {
    let m = |n: i32| match n {
        1 => "دقيقة".to_string(),
        2 => "دقيقتين".to_string(),
        3..=10 => format!("{n} دقائق"),
        _ => format!("{n} دقيقة"),
    };
    let h = |n: i32| match n {
        1 => "ساعة".to_string(),
        2 => "ساعتين".to_string(),
        3..=10 => format!("{n} ساعات"),
        _ => format!("{n} ساعة"),
    };
    let (hh, mm) = (mins / 60, mins % 60);
    match (hh, mm) {
        (0, 0) => "الآن".into(),
        (0, mm) => format!("بعد {}", m(mm)),
        (hh, 0) => format!("بعد {}", h(hh)),
        (hh, mm) => format!("بعد {} و{}", h(hh), m(mm)),
    }
}

pub struct NextPrayer {
    cache: Box<dyn Fn() -> Value>,
}

impl NextPrayer {
    pub fn new(cache: Box<dyn Fn() -> Value>) -> NextPrayer {
        NextPrayer { cache }
    }

    /// (name, "HH:MM", minutes left, share of the wait already gone 0..1)
    fn now(&self) -> Option<(String, String, i32, f32)> {
        let c = (self.cache)();
        let t: SYSTEMTIME = unsafe { GetLocalTime() };
        let now = t.wHour as i32 * 60 + t.wMinute as i32;
        let friday = t.wDayOfWeek == 5;
        let today = &c["today"]["times"];
        let list: Vec<(String, String, i32)> = PRAYERS
            .iter()
            .filter_map(|(k, ar)| {
                let s = today[*k].as_str()?;
                let name = if *k == "dhuhr" && friday { "الجمعة" } else { ar };
                Some((name.to_string(), s.to_string(), minutes(s)?))
            })
            .collect();
        if list.is_empty() {
            return None;
        }
        let next = list.iter().position(|p| p.2 > now);
        let (name, at, at_min, prev_min) = match next {
            Some(i) => {
                let prev = if i == 0 { list[list.len() - 1].2 - 1440 } else { list[i - 1].2 };
                (list[i].0.clone(), list[i].1.clone(), list[i].2, prev)
            }
            // after the isha: tomorrow's fajr
            None => {
                let s = c["tomorrow"]["times"]["fajr"].as_str().unwrap_or("05:00").to_string();
                let m = minutes(&s).unwrap_or(300) + 1440;
                ("الفجر".to_string(), s, m, list[list.len() - 1].2)
            }
        };
        let left = at_min - now;
        let span = (at_min - prev_min).max(1) as f32;
        Some((name, at, left, ((now - prev_min) as f32 / span).clamp(0.0, 1.0)))
    }
}

impl Face for NextPrayer {
    fn size(&self) -> (f32, f32) {
        (176.0, 176.0)
    }
    fn id(&self) -> &str {
        "prayer"
    }
    fn tick(&self) -> u32 {
        // the minute changes: a redraw each 15 s is enough
        15_000
    }
    fn draw(&mut self, ctx: &Ctx) -> Result<()> {
        let rt = ctx.rt;
        let (w, h) = self.size();
        unsafe {
            // Nova's glass tint over the acrylic, and a hairline edge
            let tint = rt.CreateSolidColorBrush(&rgba(22, 26, 46, 0.42), None)?;
            let edge = rt.CreateSolidColorBrush(&rgba(255, 255, 255, 0.10), None)?;
            let card = D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: 0.5, top: 0.5, right: w - 0.5, bottom: h - 0.5 }, radiusX: 22.0, radiusY: 22.0 };
            rt.FillRoundedRectangle(&card, &tint);
            rt.DrawRoundedRectangle(&card, &edge, 1.0, None);

            let Some((name, at, left, done)) = self.now() else {
                let f = ctx.fonts.format(UI, 13.0, DWRITE_FONT_WEIGHT_NORMAL, false)?;
                let white = rt.CreateSolidColorBrush(&rgba(255, 255, 255, 0.7), None)?;
                let txt: Vec<u16> = "Prayer times loading".encode_utf16().collect();
                rt.DrawText(&txt, &f, &D2D_RECT_F { left: 0.0, top: 0.0, right: w, bottom: h }, &white, D2D1_DRAW_TEXT_OPTIONS_NONE, Default::default());
                return Ok(());
            };

            // the ring: the whole wait, and how much of it has gone
            let (cx, cy, r) = (w / 2.0, h / 2.0, 68.0);
            let track = rt.CreateSolidColorBrush(&rgba(255, 255, 255, 0.12), None)?;
            let caps = D2D1_STROKE_STYLE_PROPERTIES { startCap: D2D1_CAP_STYLE_ROUND, endCap: D2D1_CAP_STYLE_ROUND, ..Default::default() };
            let round = ctx.d2d.CreateStrokeStyle(&caps, None)?;
            rt.DrawEllipse(&D2D1_ELLIPSE { point: Vector2 { X: cx, Y: cy }, radiusX: r, radiusY: r }, &track, 6.0, &round);
            let stops = [
                D2D1_GRADIENT_STOP { position: 0.0, color: rgba(111, 140, 255, 1.0) },
                D2D1_GRADIENT_STOP { position: 1.0, color: rgba(178, 107, 255, 1.0) },
            ];
            let coll = rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?;
            let grad = rt.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: Vector2 { X: cx - r, Y: cy - r }, endPoint: Vector2 { X: cx + r, Y: cy + r } },
                None,
                &coll,
            )?;
            let sweep = (done * 359.9).max(0.5);
            let a0 = -90f32.to_radians();
            let a1 = (-90.0 + sweep).to_radians();
            let geo = ctx.d2d.CreatePathGeometry()?;
            let sink = geo.Open()?;
            sink.BeginFigure(Vector2 { X: cx + r * a0.cos(), Y: cy + r * a0.sin() }, D2D1_FIGURE_BEGIN_HOLLOW);
            sink.AddArc(&D2D1_ARC_SEGMENT {
                point: Vector2 { X: cx + r * a1.cos(), Y: cy + r * a1.sin() },
                size: D2D_SIZE_F { width: r, height: r },
                rotationAngle: 0.0,
                sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                arcSize: if sweep > 180.0 { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
            });
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
            sink.Close()?;
            rt.DrawGeometry(&geo, &grad, 6.0, &round);

            // the time, the name in calligraphy, the time left
            let white = rt.CreateSolidColorBrush(&rgba(255, 255, 255, 1.0), None)?;
            let soft = rt.CreateSolidColorBrush(&rgba(255, 255, 255, 0.62), None)?;
            let line = |text: &str, fmt: &IDWriteTextFormat, top: f32, bottom: f32, brush: &ID2D1SolidColorBrush| {
                let t: Vec<u16> = text.encode_utf16().collect();
                rt.DrawText(&t, fmt, &D2D_RECT_F { left: 8.0, top, right: w - 8.0, bottom }, brush, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, Default::default());
            };
            let f_time = ctx.fonts.format(UI, 17.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, false)?;
            let f_name = ctx.fonts.format(CALLIGRAPHY, 34.0, DWRITE_FONT_WEIGHT_BOLD, true)?;
            let f_left = ctx.fonts.format(UI, 11.5, DWRITE_FONT_WEIGHT_NORMAL, true)?;
            line(&crate::clock::show(&at), &f_time, 38.0, 60.0, &white);
            line(&name, &f_name, 58.0, 118.0, &white);
            line(&time_left_ar(left), &f_left, 114.0, 134.0, &soft);
            let _ = HSTRING::new();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arabic_time_left() {
        assert_eq!(time_left_ar(1), "بعد دقيقة");
        assert_eq!(time_left_ar(2), "بعد دقيقتين");
        assert_eq!(time_left_ar(5), "بعد 5 دقائق");
        assert_eq!(time_left_ar(45), "بعد 45 دقيقة");
        assert_eq!(time_left_ar(60), "بعد ساعة");
        assert_eq!(time_left_ar(125), "بعد ساعتين و5 دقائق");
        assert_eq!(time_left_ar(0), "الآن");
    }
}
