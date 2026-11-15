// (1.8.7) Settings › Show on the taskbar: what the Nova strip on the taskbar says, decided here;
// the drawing is nova-render/src/taskbar.rs and the place nova-sys/src/taskbar.rs. Coming soon:
// off in Settings for now (nova-core features.rs keeps taskbarStrip off).
//
// What shows, first that applies:
//   1. a prayer has just come (its first 30 s)   "حيّ على الفلاح"
//   2. media playing (unless a prayer is 3 min away or less)   the bars or ▶, the title, the time reached
//   3. a prayer within 4 h, in turns with 4 every 15 s (its last 3 min: the prayer only)                        "العصر بعد 25 د" over its filling background
//   4. otherwise                                  CPU and RAM
// Called with each nova-sys line (every 500 ms): nothing else wakes for it.
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{atomic::Ordering, Arc, Mutex},
    time::Instant,
};

use nova_render::{
    clock::{self, Now},
    taskbar::{self as strip, Content, Look, Mood, Place},
};
use serde_json::Value;

use crate::App;

const HAYYA_S: i32 = 30;
const PRAYER_FIRST_S: i32 = 3 * 60;
const PRAYER_SOON_S: i32 = 4 * 3600;
const TURN_S: u64 = 15;

struct State {
    slot: Arc<Mutex<Option<nova_sys::taskbar::Slot>>>,
    cpu: (u64, u64),
    perf: (u32, u32),
    perf_at: Instant,
    /// the media's position as last reported, and when (its time goes on from there while it plays)
    pos: (String, f64, Instant),
    shown: bool,
    /// today's coffee: until when it shows (the day's first use of the PC, 10 min)
    coffee_until: Option<Instant>,
}

thread_local! {
    static ST: RefCell<Option<State>> = const { RefCell::new(None) };
    static TEST_T0: std::cell::OnceCell<Instant> = const { std::cell::OnceCell::new() };
}

fn clock_text(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// "العصر بعد 2:30" above an hour, "العصر بعد 45 د" from an hour down, "بعد ثوانٍ" its last minute.
fn prayer_text(name: &str, left_s: i32) -> String {
    let mins = (left_s + 59) / 60;
    if left_s < 60 {
        format!("{name} بعد ثوانٍ")
    } else if mins <= 60 {
        format!("{name} بعد {mins} د")
    } else {
        format!("{name} بعد {}:{:02}", mins / 60, mins % 60)
    }
}

/// Every 15 s the prayer's turn, then performance's (by the clock, so it doesn't drift).
fn turn_is_prayer() -> bool {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    (s / TURN_S) % 2 == 0
}

/// The next prayer, and one that has just come: (name, seconds to it), (name of the one now).
fn prayer_now(cache: &Value) -> (Option<(String, i32)>, Option<String>) {
    prayer_at(cache, &Now::get().at_place(cache))
}

fn prayer_at(cache: &Value, now: &Now) -> (Option<(String, i32)>, Option<String>) {
    let now = *now;
    let secs = (now.h * 3600 + now.mi * 60 + now.s) as i32;
    let came = clock::cache_day(cache, &now).and_then(|day| clock::prayers_of(day, now.wd == 5).into_iter().find(|p| (0..HAYYA_S).contains(&(secs - p.min * 60))).map(|p| p.name));
    let next = clock::schedule(cache, &now).and_then(|s| {
        let p = s.list.get(s.next)?;
        let at = p.min * 60 + if s.tomorrow { 86_400 } else { 0 };
        Some((p.name.clone(), at - secs))
    });
    (next, came)
}

/// The eyes' mood: asleep from midnight to 5, a coffee for the day's first 10 minutes at the PC
/// (once a day, kept across restarts in taskbar-coffee.txt), awake otherwise.
fn mood_now(s: &mut State, line: &Value) -> Mood {
    if let Some(m) = crate::arg("--test-taskbar-mood") {
        return match m.as_str() {
            "sleep" => Mood::Sleep,
            "coffee" => Mood::Coffee,
            _ => Mood::Normal,
        };
    }
    let now = Now::get();
    if now.h < 5 {
        return Mood::Sleep;
    }
    if let Some(until) = s.coffee_until {
        if Instant::now() < until {
            return Mood::Coffee;
        }
    }
    // someone at the PC, and no coffee yet today
    let at_pc = line["idle"].as_f64().is_some_and(|i| i < 60.0);
    if at_pc && s.coffee_until.is_none() {
        let file = crate::data_dir().join("taskbar-coffee.txt");
        let today = now.date_key();
        if std::fs::read_to_string(&file).ok().as_deref().map(str::trim) != Some(today.as_str()) {
            let _ = std::fs::write(&file, &today);
            s.coffee_until = Some(Instant::now() + std::time::Duration::from_secs(10 * 60));
            return Mood::Coffee;
        }
        // had it today already: not asked again until Nova starts again
        s.coffee_until = Some(Instant::now());
    }
    Mood::Normal
}

pub fn on_line(app: &Rc<RefCell<App>>, line: &Value) {
    let (on, still, theme, prayer_cache) = {
        let a = app.borrow();
        let f = &a.core.features;
        (f.bool("taskbarStrip"), f.bool("noAnimations"), f.get()["theme"].as_str().unwrap_or("").to_string(), a.prayer.snapshot())
    };
    nova_sys::taskbar::WANTED.store(on, Ordering::Relaxed);
    ST.with(|st| {
        let mut st = st.borrow_mut();
        if !on {
            if st.as_ref().is_some_and(|s| s.shown) {
                strip::close();
                if let Some(s) = st.as_mut() {
                    s.shown = false;
                }
            }
            return;
        }
        let s = st.get_or_insert_with(|| State { slot: nova_sys::taskbar::spawn(), cpu: nova_sys::sysinfo::cpu_times(), perf: (0, 0), perf_at: Instant::now() - std::time::Duration::from_secs(10), pos: (String::new(), 0.0, Instant::now()), shown: false, coffee_until: None });
        let slot = *s.slot.lock().unwrap();
        // no taskbar to sit on, or something full screen in front
        let Some(slot) = slot.filter(|_| line["fs"] != true) else {
            if s.shown {
                strip::hide();
                s.shown = false;
            }
            return;
        };
        if s.perf_at.elapsed().as_secs_f32() >= 2.0 {
            let (idle, total) = nova_sys::sysinfo::cpu_times();
            let (di, dt) = (idle.saturating_sub(s.cpu.0), total.saturating_sub(s.cpu.1));
            let cpu = if dt > 0 { (100 - (di * 100 / dt).min(100)) as u32 } else { s.perf.0 };
            let (tot, free) = nova_sys::sysinfo::memory();
            let ram = if tot > 0.0 { ((1.0 - free / tot) * 100.0).round() as u32 } else { 0 };
            s.cpu = (idle, total);
            s.perf = (cpu, ram);
            s.perf_at = Instant::now();
        }
        let (mut next, mut came) = prayer_now(&prayer_cache);
        // tests: --test-taskbar-prayer S (a prayer S seconds away; negative: it came -S s ago),
        // --test-taskbar-media "title" (a song playing)
        if let Some(t) = crate::arg("--test-taskbar-prayer").and_then(|v| v.parse::<i32>().ok()) {
            let started = TEST_T0.with(|t0| *t0.get_or_init(Instant::now));
            let left = t - started.elapsed().as_secs() as i32;
            next = Some(("العصر".into(), left));
            came = (-HAYYA_S < left && left <= 0).then(|| "العصر".into());
        }
        let test_media = crate::arg("--test-taskbar-media");
        let playing = test_media.is_some() || (line["has"] == true && line["playing"] == true);
        let title = test_media.clone().unwrap_or_else(|| line["title"].as_str().unwrap_or("").trim().to_string());
        let pos = line["pos"].as_f64().unwrap_or(0.0);
        if title != s.pos.0 || (pos - s.pos.1).abs() > 0.01 {
            s.pos = (title.clone(), pos, Instant::now());
        }
        let content = if let Some(name) = came {
            let _ = name;
            Content::Hayya { text: "حيّ على الفلاح".into() }
        } else if playing && !title.is_empty() && !next.as_ref().is_some_and(|(_, left)| *left <= PRAYER_FIRST_S) {
            let app_name = line["app"].as_str().unwrap_or("").to_lowercase();
            let video = ["chrome", "msedge", "firefox", "brave", "opera", "vivaldi", "vlc", "mpc", "potplayer", "movies", "video"].iter().any(|b| app_name.contains(b));
            let at = s.pos.1 + s.pos.2.elapsed().as_secs_f64();
            Content::Media { title, time: clock_text(at), video, playing }
        // a prayer within 4 h takes turns with performance every 15 s; its last 3 min it stays
        } else if let Some((name, left)) = next.filter(|(_, left)| (0..=PRAYER_SOON_S).contains(left) && (*left <= PRAYER_FIRST_S || turn_is_prayer())) {
            let text = prayer_text(&name, left);
            Content::Prayer { text, fill: 1.0 - left as f32 / PRAYER_SOON_S as f32 }
        } else {
            Content::Perf { cpu: s.perf.0, ram: s.perf.1 }
        };
        let accent = if theme == "madar" { (147.0 / 255.0, 228.0 / 255.0, 200.0 / 255.0) } else { (140.0 / 255.0, 200.0 / 255.0, 1.0) };
        let place = Place { x_min: slot.x_min, x_max: slot.x_max, at_end: slot.at_end, top: slot.taskbar.top, bottom: slot.taskbar.bottom, dpi: slot.dpi };
        let mood = mood_now(s, line);
        strip::show(&place, &content, &Look { light: slot.light, accent, still }, mood);
        // --test-taskbar-card quran|dhikr: a card over the strip once it's up
        if !s.shown {
            if let Some(k) = crate::arg("--test-taskbar-card") {
                let c = if k == "dhikr" {
                    serde_json::json!({ "name": "ذكر", "text": "سبحان الله وبحمده، سبحان الله العظيم", "ref": "", "quran": false, "ms": 60000 })
                } else {
                    serde_json::json!({ "name": "آية", "text": "الَّذِينَ آمَنُوا وَتَطْمَئِنُّ قُلُوبُهُم بِذِكْرِ اللَّهِ ۗ أَلَا بِذِكْرِ اللَّهِ تَطْمَئِنُّ الْقُلُوبُ ٢٨", "ref": "سورة الرعد", "quran": true, "ms": 60000 })
                };
                s.shown = true;
                card(&c);
            }
        }
        s.shown = true;
    });
}

/// A reminder card from the page (views/reminders.js, the strip on): over the strip.
pub fn card(v: &Value) {
    static FONT: std::sync::Once = std::sync::Once::new();
    FONT.call_once(|| {
        if let Some((bytes, _)) = nova_assets::get("fonts/KFGQPC-Hafs-V30.ttf") {
            strip::set_quran_font(bytes);
        }
    });
    let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
    let c = strip::Card { kind: s("name"), text: s("text"), reference: s("ref"), quran: v["quran"] == true, ms: v["ms"].as_u64().unwrap_or(10_000).clamp(2_000, 120_000) as u32 };
    if c.text.is_empty() {
        return;
    }
    let _ = strip::card(&c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn at(h: u32, mi: u32, s: u32) -> Now {
        // Friday 14 Nov 2026, Cairo (UTC+2)
        Now { y: 2026, mo: 11, d: 14, wd: 6, h, mi, s, utc_ms: 0, offset_ms: 2 * 3_600_000 }
    }

    #[test]
    fn the_prayer_times_on_the_strip() {
        let times = |m: &str| json!({ "fajr": "04:50", "sunrise": "06:15", "dhuhr": "11:40", "asr": "14:40", "maghrib": m, "isha": "18:20" });
        let c = json!({ "today": { "date": "2026-11-14", "times": times("17:00") }, "tomorrow": { "date": "2026-11-15", "times": times("16:59") } });
        // past midnight: the fajr 3 h 57 min away (too far for the strip)
        assert_eq!(prayer_at(&c, &at(0, 53, 0)).0, Some(("الفجر".into(), (4 * 60 + 50 - 53) * 60)));
        // 1 h 55 min before it: within the two hours
        let (next, came) = prayer_at(&c, &at(2, 55, 0));
        assert_eq!((next.map(|n| n.1), came), (Some(115 * 60), None));
        // its first 30 s: حيّ على الفلاح
        assert_eq!(prayer_at(&c, &at(4, 50, 10)).1, Some("الفجر".into()));
        assert_eq!(prayer_at(&c, &at(4, 50, 31)).1, None);
        // the time left: hours and minutes above an hour, minutes from an hour down
        assert_eq!(prayer_text("العصر", 150 * 60), "العصر بعد 2:30");
        assert_eq!(prayer_text("العصر", 61 * 60), "العصر بعد 1:01");
        assert_eq!(prayer_text("العصر", 60 * 60), "العصر بعد 60 د");
        assert_eq!(prayer_text("العصر", 45 * 60 - 10), "العصر بعد 45 د");
        assert_eq!(prayer_text("العصر", 30), "العصر بعد ثوانٍ");
        // after the isha: tomorrow's fajr
        let (next, _) = prayer_at(&c, &at(22, 0, 0));
        assert_eq!(next.map(|n| n.0), Some("الفجر".into()));
    }
}
