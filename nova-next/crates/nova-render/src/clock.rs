//! The date and time the faces show: the PC's local clock, Arabic names, the Hijri date and the
//! prayer times from prayer-cache.json (the same file the island reads).
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;
use windows::Win32::{Foundation::SYSTEMTIME, System::SystemInformation::{GetLocalTime, GetSystemTime}};

pub const DAYS: [&str; 7] = ["الأحد", "الإثنين", "الثلاثاء", "الأربعاء", "الخميس", "الجمعة", "السبت"];
pub const MONTHS: [&str; 12] = ["يناير", "فبراير", "مارس", "أبريل", "مايو", "يونيو", "يوليو", "أغسطس", "سبتمبر", "أكتوبر", "نوفمبر", "ديسمبر"];
pub const PRAYERS: [(&str, &str); 5] = [("fajr", "الفجر"), ("dhuhr", "الظهر"), ("asr", "العصر"), ("maghrib", "المغرب"), ("isha", "العشاء")];

/// Days since 1970-01-01 of a civil date.
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}
/// The civil date of a day count since 1970-01-01.
pub fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((yoe + era * 400 + if m <= 2 { 1 } else { 0 }) as i32, m, d)
}
pub fn leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
pub fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        2 if leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}
/// 0 = Sunday
pub fn weekday(y: i32, m: u32, d: u32) -> u32 {
    ((days_from_civil(y as i64, m as i64, d as i64) + 4).rem_euclid(7)) as u32
}

/// The local wall clock now.
#[derive(Clone, Copy)]
pub struct Now {
    pub y: i32,
    pub mo: u32,
    pub d: u32,
    /// 0 = Sunday
    pub wd: u32,
    pub h: u32,
    pub mi: u32,
    pub s: u32,
    /// UTC milliseconds since 1970 (what tasks.json keeps)
    pub utc_ms: i64,
    /// local time minus UTC, milliseconds
    pub offset_ms: i64,
}

fn ms_of(t: &SYSTEMTIME) -> i64 {
    days_from_civil(t.wYear as i64, t.wMonth as i64, t.wDay as i64) * 86_400_000 + (t.wHour as i64 * 3600 + t.wMinute as i64 * 60 + t.wSecond as i64) * 1000 + t.wMilliseconds as i64
}

impl Now {
    pub fn get() -> Now {
        let (l, u) = unsafe { (GetLocalTime(), GetSystemTime()) };
        let utc_ms = ms_of(&u);
        // whole minutes: the two reads can straddle a millisecond
        let offset_ms = ((ms_of(&l) - utc_ms) as f64 / 60_000.0).round() as i64 * 60_000;
        Now { y: l.wYear as i32, mo: l.wMonth as u32, d: l.wDay as u32, wd: l.wDayOfWeek as u32, h: l.wHour as u32, mi: l.wMinute as u32, s: l.wSecond as u32, utc_ms, offset_ms }
    }
    pub fn minutes(&self) -> i32 {
        (self.h * 60 + self.mi) as i32
    }
    /// "2026-11-09"
    pub fn date_key(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.y, self.mo, self.d)
    }
    /// The UTC milliseconds today starts and ends at (local midnight to midnight).
    pub fn today_utc(&self) -> (i64, i64) {
        let start = days_from_civil(self.y as i64, self.mo as i64, self.d as i64) * 86_400_000 - self.offset_ms;
        (start, start + 86_400_000)
    }
    /// (1.8.7) The same moment on the prayer place's clock (prayer-cache.json's "utcOffset", in
    /// minutes): the city can be in another time zone than the PC, and its day, times and
    /// countdown are the city's. Unchanged for a cache without it (written before 1.8.7).
    pub fn at_place(&self, c: &Value) -> Now {
        let off = ["today", "tomorrow"].iter().find_map(|k| c[*k]["utcOffset"].as_i64());
        let Some(off) = off.filter(|o| o.abs() <= 18 * 60) else { return *self };
        let offset_ms = off * 60_000;
        let local = self.utc_ms + offset_ms;
        let days = local.div_euclid(86_400_000);
        let (y, mo, d) = civil_from_days(days);
        let sec = local.rem_euclid(86_400_000) / 1000;
        Now { y, mo, d, wd: weekday(y, mo, d), h: (sec / 3600) as u32, mi: (sec / 60 % 60) as u32, s: (sec % 60) as u32, utc_ms: self.utc_ms, offset_ms }
    }
    /// The local time of a UTC millisecond time, as Settings › Time format says ("21:05" / "9:05 م").
    pub fn hhmm(&self, utc_ms: i64) -> String {
        let m = (utc_ms + self.offset_ms).rem_euclid(86_400_000) / 60_000;
        hm(m as u32 / 60, m as u32 % 60)
    }
}

/// Settings › General › Time format (the "clock24" feature): 24 hours, or 12 with ص / م.
static H24: AtomicBool = AtomicBool::new(false);

/// Sets the time format; true when it changed.
pub fn set_24(on: bool) -> bool {
    H24.swap(on, Ordering::Relaxed) != on
}

/// "21:05" in 24 hours, "9:05 م" in 12 (ص before noon, م after; 0:30 is "12:30 ص").
pub fn hm(h: u32, m: u32) -> String {
    hm_as(h, m, H24.load(Ordering::Relaxed))
}

fn hm_as(h: u32, m: u32, h24: bool) -> String {
    if h24 {
        return format!("{:02}:{:02}", h % 24, m);
    }
    let h = h % 24;
    let h12 = if h % 12 == 0 { 12 } else { h % 12 };
    format!("{h12}:{m:02} {}", if h < 12 { "ص" } else { "م" })
}

/// A "HH:MM" from prayer-cache.json in the time format (as it was when it isn't one).
pub fn show(hhmm: &str) -> String {
    match minutes(hhmm) {
        Some(m) if (0..1440).contains(&m) => hm(m as u32 / 60, m as u32 % 60),
        _ => hhmm.to_string(),
    }
}

/// The same without ص / م (where a time stands in a narrow box).
pub fn show_short(hhmm: &str) -> String {
    let s = show(hhmm);
    s.trim_end_matches([' ', 'ص', 'م']).to_string()
}

pub fn minutes(hhmm: &str) -> Option<i32> {
    let (h, m) = hhmm.split_once(':')?;
    Some(h.trim().parse::<i32>().ok()? * 60 + m.trim().parse::<i32>().ok()?)
}

/// The cache's day for today's date (the app writes today and tomorrow).
pub fn cache_day<'a>(c: &'a Value, now: &Now) -> Option<&'a Value> {
    let key = now.date_key();
    ["today", "tomorrow"].iter().map(|k| &c[*k]).find(|d| d["date"].as_str() == Some(key.as_str())).or_else(|| c.get("today"))
}

/// Today's Hijri date: (day, month name, year).
pub fn hijri(c: &Value, now: &Now) -> Option<(u32, String, u32)> {
    let s = cache_day(c, now)?["hijri"].as_str()?;
    let mut parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }
    let day = parts.remove(0).parse().ok()?;
    let year = parts.pop()?.parse().ok()?;
    Some((day, parts.join(" "), year))
}

/// One prayer: its Arabic name (الجمعة for the Friday noon), "HH:MM", minutes after midnight.
#[derive(Clone)]
pub struct Prayer {
    pub name: String,
    pub at: String,
    pub min: i32,
}

/// A day's five prayers from a cache day.
pub fn prayers_of(day: &Value, friday: bool) -> Vec<Prayer> {
    PRAYERS
        .iter()
        .filter_map(|(k, ar)| {
            let s = day["times"][*k].as_str()?;
            let name = if *k == "dhuhr" && friday { "الجمعة" } else { ar };
            Some(Prayer { name: name.to_string(), at: s.to_string(), min: minutes(s)? })
        })
        .collect()
}

/// The day's list (today's, or tomorrow's after the isha), which one is next, and the minutes left.
pub struct Schedule {
    pub list: Vec<Prayer>,
    pub next: usize,
    pub left: i32,
    /// the list is tomorrow's (it's after the isha)
    pub tomorrow: bool,
}

pub fn schedule(c: &Value, now: &Now) -> Option<Schedule> {
    let today = cache_day(c, now)?;
    let list = prayers_of(today, now.wd == 5);
    if list.len() < 5 {
        return None;
    }
    let m = now.minutes();
    if let Some(i) = list.iter().position(|p| p.min > m) {
        let left = list[i].min - m;
        return Some(Schedule { list, next: i, left, tomorrow: false });
    }
    let tom = prayers_of(&c["tomorrow"], now.wd == 4);
    let tom = if tom.len() == 5 { tom } else { list };
    let left = tom[0].min + 1440 - m;
    Some(Schedule { list: tom, next: 0, left, tomorrow: true })
}

/// "بعد 45 دقيقة", "بعد ساعتين و5 دقائق"
pub fn time_left_ar(mins: i32) -> String {
    crate::faces::time_left_ar(mins)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_prayer_place_has_its_own_clock() {
        for days in [-1, 0, 59, 20_405, 20_770, 21_000] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y as i64, m as i64, d as i64), days);
        }
        // the PC in Cairo at 22:30 on 12 Nov 2026 (20:30 UTC); the place is Tokyo: 05:30 on the 13th
        let utc = days_from_civil(2026, 11, 12) * 86_400_000 + (20 * 60 + 30) * 60_000;
        let pc = Now { y: 2026, mo: 11, d: 12, wd: 4, h: 22, mi: 30, s: 0, utc_ms: utc, offset_ms: 120 * 60_000 };
        let tokyo = pc.at_place(&serde_json::json!({ "today": { "date": "2026-11-13", "utcOffset": 540 } }));
        assert_eq!((tokyo.date_key().as_str(), tokyo.h, tokyo.mi, tokyo.wd), ("2026-11-13", 5, 30, 5));
        assert_eq!(tokyo.today_utc().0, days_from_civil(2026, 11, 13) * 86_400_000 - 540 * 60_000);
        // an older cache: the PC's clock
        assert_eq!(pc.at_place(&serde_json::json!({ "today": { "date": "2026-11-12" } })).date_key(), "2026-11-12");
    }

    #[test]
    fn twelve_and_twenty_four_hours() {
        assert_eq!(hm_as(0, 30, false), "12:30 ص");
        assert_eq!(hm_as(5, 7, false), "5:07 ص");
        assert_eq!(hm_as(12, 0, false), "12:00 م");
        assert_eq!(hm_as(21, 5, false), "9:05 م");
        assert_eq!(hm_as(21, 5, true), "21:05");
        assert_eq!(hm_as(5, 7, true), "05:07");
    }
}
