//! Widgets that do things: today's tasks (9), the tasbih (10), the focus timer (15), the water
//! bottle (18) and the PC's pulse (19).
use serde_json::{json, Value};
use windows::{
    core::{Interface, Result},
    Win32::{
        Foundation::FILETIME,
        Graphics::Direct2D::{Common::*, *},
        System::{
            Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS},
            SystemInformation::{GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX},
            Threading::GetSystemTimes,
        },
    },
};

use crate::{
    clock::Now,
    paint::*,
    state::Store,
    widget::{Ctx, Face},
};

const GREEN: (u8, u8, u8) = (93, 211, 158);

/// The Tasks plugin's tag colours (pages/tasks/tasks.css).
fn tag_color(tag: &str) -> D2D1_COLOR_F {
    match tag {
        "urgent" => rgba(255, 69, 58, 1.0),
        "high" => rgba(255, 159, 10, 1.0),
        "low" => rgba(142, 142, 147, 1.0),
        _ => rgba(10, 132, 255, 1.0),
    }
}

/// One row of today's list.
pub struct Row {
    pub title: String,
    pub tag: String,
    pub time: String,
    pub done: bool,
}

/// Today's tasks from tasks.json: the open ones due by tonight (late ones too), then the ones done
/// today; and (done today, all of today's).
pub fn today(tasks: &Value, now: &Now) -> (Vec<Row>, usize, usize) {
    let (start, end) = now.today_utc();
    let mut open: Vec<(i64, Row)> = vec![];
    let mut done: Vec<(i64, Row)> = vec![];
    for t in tasks.as_array().into_iter().flatten() {
        let title = t["title"].as_str().unwrap_or("").to_string();
        let tag = t["tag"].as_str().unwrap_or("medium").to_string();
        // (1.8.7) tasks.rs keeps its times as JSON numbers with a fraction part (1.7's): as_i64
        // said None for every one of them, so the widget showed no task at all
        let ms = |v: &Value| v.as_f64().filter(|x| x.is_finite()).map(|x| x as i64);
        let due = ms(&t["due"]);
        if t["done"].as_bool() == Some(true) {
            let at = ms(&t["doneAt"]).unwrap_or(0);
            if (start..end).contains(&at) {
                done.push((at, Row { title, tag, time: String::new(), done: true }));
            }
        } else if let Some(due) = due.filter(|d| *d < end) {
            let time = if due < start { "متأخرة".to_string() } else { now.hhmm(due) };
            open.push((due, Row { title, tag, time, done: false }));
        }
    }
    open.sort_by_key(|r| r.0);
    done.sort_by_key(|r| r.0);
    let (n_done, n_all) = (done.len(), open.len() + done.len());
    (open.into_iter().chain(done).map(|r| r.1).collect(), n_done, n_all)
}

/// 9: a ring of today's done share, and the list.
pub struct Tasks {
    tasks: Box<dyn Fn() -> Value>,
}
impl Tasks {
    pub fn new(tasks: Box<dyn Fn() -> Value>) -> Tasks {
        Tasks { tasks }
    }
}
impl Face for Tasks {
    fn size(&self) -> (f32, f32) {
        (370.0, 176.0)
    }
    fn id(&self) -> &str {
        "tasks"
    }
    fn tick(&self) -> u32 {
        20_000
    }
    fn draw(&mut self, ctx: &Ctx) -> Result<()> {
        let p = P::new(ctx);
        let (w, h) = self.size();
        p.card(w, h)?;
        let now = Now::get();
        let (rows, n_done, n_all) = today(&(self.tasks)(), &now);
        let full: ID2D1Brush = p.solid(white(1.0))?.cast()?;
        let soft: ID2D1Brush = p.solid(white(0.62))?.cast()?;

        // the ring, on the right
        let (cx, cy) = (w - 18.0 - 48.0, 72.0);
        p.stroke_circle(cx, cy, 34.0, &p.solid(white(0.12))?.cast()?, 6.0);
        if n_done > 0 {
            p.ring_arc(cx, cy, 34.0, n_done as f32 / n_all.max(1) as f32, &p.solid(c3(GREEN, 1.0))?.cast()?, 6.0)?;
        }
        p.text(&format!("{n_done}/{n_all}"), ui(20.0, 700), rect(cx - 34.0, cy - 16.0, cx + 34.0, cy + 16.0), Align::Center, &full)?;
        p.text("مهام اليوم", ruqaa(18.0), rect(cx - 52.0, 112.0, cx + 52.0, 142.0), Align::Center, &full)?;

        // the list, on the left
        let (l, r) = (18.0, w - 18.0 - 96.0 - 18.0);
        if rows.is_empty() {
            return p.text("لا مهام لليوم", ui(13.0, 400), rect(l, 0.0, r, h), Align::Center, &soft);
        }
        let shown = rows.len().min(4);
        let (lh, gap) = (20.0, 9.0);
        let top = (h - (shown as f32 * lh + (shown - 1) as f32 * gap)) / 2.0;
        for (i, row) in rows.iter().take(shown).enumerate() {
            let y = top + i as f32 * (lh + gap);
            let dot = if row.done { c3(GREEN, 1.0) } else { tag_color(&row.tag) };
            p.fill_circle(r - 4.0, y + lh / 2.0, 4.0, &p.solid(dot)?.cast()?);
            let tw = if row.time.is_empty() { 0.0 } else { 46.0 };
            p.text_x(&row.title, ui(13.0, 400), rect(l + tw, y, r - 16.0, y + lh), Align::Right, if row.done { &soft } else { &full }, row.done)?;
            if !row.time.is_empty() {
                p.text(&row.time, ui(11.0, 400), rect(l, y, l + tw, y + lh), Align::Left, &soft)?;
            }
        }
        Ok(())
    }
}

/// 10: the tasbih: 33 beads round the edge; a click counts one; past 33 the next phrase.
pub struct Tasbih {
    store: Store,
}
const PHRASES: [&str; 3] = ["سبحان الله", "الحمد لله", "الله أكبر"];
impl Tasbih {
    pub fn new(store: Store) -> Tasbih {
        Tasbih { store }
    }
    fn get(&self) -> (usize, u32) {
        let v = self.store.get("tasbih");
        (v["phrase"].as_u64().unwrap_or(0) as usize % PHRASES.len(), v["count"].as_u64().unwrap_or(0).min(33) as u32)
    }
}
impl Face for Tasbih {
    fn size(&self) -> (f32, f32) {
        (176.0, 176.0)
    }
    fn id(&self) -> &str {
        "tasbih"
    }
    fn tick(&self) -> u32 {
        3_600_000
    }
    fn click(&mut self, _x: f32, _y: f32) -> bool {
        let (mut ph, mut n) = self.get();
        if n >= 33 {
            ph = (ph + 1) % PHRASES.len();
            n = 1;
        } else {
            n += 1;
        }
        self.store.set("tasbih", json!({ "phrase": ph, "count": n }));
        true
    }
    fn draw(&mut self, ctx: &Ctx) -> Result<()> {
        let p = P::new(ctx);
        let (w, h) = self.size();
        p.card(w, h)?;
        let (ph, n) = self.get();
        let on: ID2D1Brush = p.solid(rgba(185, 166, 255, 1.0))?.cast()?;
        let off: ID2D1Brush = p.solid(white(0.2))?.cast()?;
        for i in 0..33 {
            let a = (i as f32 * 360.0 / 33.0).to_radians();
            let (x, y) = (88.0 + 72.0 * a.sin(), 88.0 - 72.0 * a.cos());
            if i < n { p.fill_circle(x, y, 3.6, &on) } else { p.fill_circle(x, y, 2.6, &off) }
        }
        p.text(PHRASES[ph], ruqaa(24.0), rect(10.0, 48.0, w - 10.0, 86.0), Align::Center, &p.solid(white(1.0))?.cast()?)?;
        p.text(&n.to_string(), ui(26.0, 700), rect(0.0, 86.0, w, 118.0), Align::Center, &p.solid(white(1.0))?.cast()?)?;
        p.text("من 33 · اضغط للعد", ui(11.0, 400), rect(0.0, 118.0, w, 134.0), Align::Center, &p.solid(white(0.62))?.cast()?)
    }
}

/// 15: a focus session of the length the user sets (− and + beside the time, 5 minutes to 3
/// hours, kept); a click elsewhere starts, pauses and goes on. When the time is up: 30 seconds
/// of alarm, a flashing ring, and the widget above the windows to be seen; a click stops it.
pub struct Focus {
    store: Store,
    /// ms left when paused or idle
    left: u64,
    /// the tick count it ends at, while running
    ends: Option<u64>,
    /// the tick count the alarm started at, while it sounds
    alarm: Option<u64>,
}
const STEP_MIN: u64 = 5;
const MAX_MIN: u64 = 180;
const ALARM_MS: u64 = 30_000;
/// the − and + buttons: centres (DIPs) and how near a click counts
const MINUS: (f32, f32) = (38.0, 94.0);
const PLUS: (f32, f32) = (138.0, 94.0);
const BUTTON_R: f32 = 12.0;

fn now_ms() -> u64 {
    unsafe { GetTickCount64() }
}

/// The alarm sound, looping (Windows' own Alarm01), or silence.
fn sound(on: bool) {
    use windows::{core::PCWSTR, Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_LOOP, SND_NODEFAULT}};
    unsafe {
        if on {
            let path = format!("{}\\Media\\Alarm01.wav", std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into()));
            let w: Vec<u16> = path.encode_utf16().chain([0]).collect();
            let _ = PlaySoundW(PCWSTR(w.as_ptr()), None, SND_FILENAME | SND_ASYNC | SND_LOOP | SND_NODEFAULT);
        } else {
            let _ = PlaySoundW(PCWSTR::null(), None, SND_ASYNC);
        }
    }
}

impl Focus {
    pub fn new(store: Store) -> Focus {
        let mut f = Focus { store, left: 0, ends: None, alarm: None };
        f.left = f.full();
        f
    }
    /// the session's length (ms), as set
    fn full(&self) -> u64 {
        // tests: a length in seconds ("seconds" in widgets-state.json), to see the alarm soon
        if let Some(t) = self.store.get("focus")["seconds"].as_u64() {
            return t * 1000;
        }
        self.minutes() * 60_000
    }
    fn minutes(&self) -> u64 {
        self.store.get("focus")["minutes"].as_u64().unwrap_or(25).clamp(STEP_MIN, MAX_MIN)
    }
    fn remaining(&self) -> u64 {
        match self.ends {
            Some(e) => e.saturating_sub(now_ms()),
            None => self.left,
        }
    }
    fn stop_alarm(&mut self) {
        if self.alarm.take().is_some() {
            sound(false);
        }
        self.left = self.full();
    }
}
impl Drop for Focus {
    fn drop(&mut self) {
        if self.alarm.is_some() {
            sound(false);
        }
    }
}
impl Face for Focus {
    fn size(&self) -> (f32, f32) {
        (176.0, 176.0)
    }
    fn id(&self) -> &str {
        "focus"
    }
    fn tick(&self) -> u32 {
        if self.alarm.is_some() {
            500
        } else if self.ends.is_some() {
            1000
        } else {
            3_600_000
        }
    }
    fn on_top(&self) -> bool {
        self.alarm.is_some()
    }
    fn click(&mut self, x: f32, y: f32) -> bool {
        if self.alarm.is_some() {
            self.stop_alarm();
            return true;
        }
        let near = |c: (f32, f32)| (x - c.0).hypot(y - c.1) <= BUTTON_R + 4.0;
        // not running: − and + set the length (and start the session over at it)
        if self.ends.is_none() && (near(MINUS) || near(PLUS)) {
            let m = self.minutes();
            let m = if near(PLUS) { (m + STEP_MIN).min(MAX_MIN) } else { m.saturating_sub(STEP_MIN).max(STEP_MIN) };
            self.store.set("focus", json!({ "minutes": m }));
            self.left = self.full();
            return true;
        }
        match self.ends {
            // a pause: what's left is kept
            Some(_) => {
                self.left = self.remaining();
                self.ends = None;
            }
            None => self.ends = Some(now_ms() + self.left.max(1000)),
        }
        true
    }
    fn draw(&mut self, ctx: &Ctx) -> Result<()> {
        let p = P::new(ctx);
        let (w, h) = self.size();
        p.card(w, h)?;
        // the time is up: the alarm, for 30 seconds
        if self.ends.is_some() && self.remaining() == 0 {
            self.ends = None;
            self.left = 0;
            self.alarm = Some(now_ms());
            sound(true);
        }
        if let Some(t) = self.alarm {
            if now_ms() - t >= ALARM_MS {
                self.stop_alarm();
            }
        }
        let full: ID2D1Brush = p.solid(white(1.0))?.cast()?;
        let soft: ID2D1Brush = p.solid(white(0.62))?.cast()?;
        let orange = rgba(255, 159, 67, 1.0);
        p.stroke_circle(88.0, 88.0, 68.0, &p.solid(white(0.12))?.cast()?, 5.0);

        if let Some(t) = self.alarm {
            // a ring that flashes, twice a second
            let on = ((now_ms() - t) / 500) % 2 == 0;
            let mut c = orange;
            c.a = if on { 1.0 } else { 0.35 };
            p.stroke_circle(88.0, 88.0, 68.0, &p.solid(c)?.cast()?, 7.0);
            p.text("انتهى الوقت", ruqaa(22.0), rect(0.0, 56.0, w, 92.0), Align::Center, &full)?;
            return p.text("اضغط للإيقاف", ui(12.0, 600), rect(0.0, 96.0, w, 116.0), Align::Center, &p.solid(orange)?.cast()?);
        }

        let r = self.remaining();
        let running = self.ends.is_some();
        if r > 0 {
            p.ring_arc(88.0, 88.0, 68.0, r as f32 / self.full().max(1) as f32, &p.solid(orange)?.cast()?, 5.0)?;
        }
        let secs = r.div_ceil(1000);
        p.text("تركيز", ruqaa(18.0), rect(0.0, 44.0, w, 74.0), Align::Center, &full)?;
        let time = if secs >= 3600 { format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60) } else { format!("{:02}:{:02}", secs / 60, secs % 60) };
        p.text(&time, ui(if secs >= 3600 { 18.0 } else { 26.0 }, 600), rect(52.0, 76.0, w - 52.0, 112.0), Align::Center, &full)?;
        // − and + while it isn't running
        if !running {
            let disc: ID2D1Brush = p.solid(white(0.10))?.cast()?;
            for (c, sign) in [(MINUS, "−"), (PLUS, "+")] {
                p.fill_circle(c.0, c.1, BUTTON_R, &disc);
                p.text(sign, ui(16.0, 600), rect(c.0 - BUTTON_R, c.1 - BUTTON_R - 1.0, c.0 + BUTTON_R, c.1 + BUTTON_R - 1.0), Align::Center, &full)?;
            }
        }
        let status = if running {
            "اضغط للإيقاف".to_string()
        } else if r == self.full() {
            format!("{} دقيقة · اضغط للبدء", self.minutes())
        } else {
            "اضغط للمتابعة".to_string()
        };
        p.text(&status, ui(11.0, 400), rect(0.0, 114.0, w, 130.0), Align::Center, &soft)
    }
}
/// 18: a bottle that fills with each glass (a click), eight a day; new each day.
pub struct Water {
    store: Store,
}
const GLASSES: u32 = 8;
impl Water {
    pub fn new(store: Store) -> Water {
        Water { store }
    }
    fn glasses(&self) -> u32 {
        let v = self.store.get("water");
        if v["date"].as_str() == Some(Now::get().date_key().as_str()) { v["glasses"].as_u64().unwrap_or(0) as u32 } else { 0 }
    }
}
impl Face for Water {
    fn size(&self) -> (f32, f32) {
        (176.0, 176.0)
    }
    fn id(&self) -> &str {
        "water"
    }
    fn tick(&self) -> u32 {
        60_000
    }
    fn click(&mut self, _x: f32, _y: f32) -> bool {
        let n = self.glasses();
        let n = if n >= GLASSES { 0 } else { n + 1 };
        self.store.set("water", json!({ "date": Now::get().date_key(), "glasses": n }));
        true
    }
    fn draw(&mut self, ctx: &Ctx) -> Result<()> {
        let p = P::new(ctx);
        let (w, h) = self.size();
        p.card(w, h)?;
        let n = self.glasses();

        // the bottle, on the right: a cap, a glass body, the water inside it
        let (bx, by) = (w - 20.0 - 54.0, (h - 128.0) / 2.0);
        p.fill_round(rect(bx + 17.0, by + 4.0, bx + 37.0, by + 16.0), 4.0, &p.solid(white(0.35))?.cast()?);
        let body = rect(bx + 4.0, by + 18.0, bx + 50.0, by + 124.0);
        p.fill_round(body, 12.0, &p.solid(white(0.08))?.cast()?);
        if n > 0 {
            let level = (body.bottom - body.top) * n as f32 / GLASSES as f32;
            unsafe {
                let d2d = ctx.d2d;
                let shape = d2d.CreateRoundedRectangleGeometry(&D2D1_ROUNDED_RECT { rect: body, radiusX: 12.0, radiusY: 12.0 })?;
                let water = d2d.CreateRectangleGeometry(&rect(body.left - 1.0, body.bottom - level, body.right + 1.0, body.bottom + 1.0))?;
                let g = p.geometry()?;
                let sink = g.Open()?;
                shape.CombineWithGeometry(&water, D2D1_COMBINE_MODE_INTERSECT, None, 0.25, &sink)?;
                sink.Close()?;
                let blue = p.linear((0.0, body.top), (0.0, body.bottom), rgba(116, 192, 252, 1.0), rgba(76, 110, 245, 1.0))?;
                p.fill_geo(&g, &blue.cast()?);
            }
        }
        p.stroke_round(rect(body.left + 0.5, body.top + 0.5, body.right - 0.5, body.bottom - 0.5), 12.0, &p.solid(white(0.3))?.cast()?, 1.0);

        // the count, right-aligned beside it
        let (l, r) = (20.0, bx - 14.0);
        let full: ID2D1Brush = p.solid(white(1.0))?.cast()?;
        let soft: ID2D1Brush = p.solid(white(0.62))?.cast()?;
        p.text("الماء", ruqaa(20.0), rect(l, 48.0, r, 80.0), Align::Right, &full)?;
        let num = n.to_string();
        let of = format!(" / {GLASSES}");
        let (nw, ow) = (p.measure(&num, ui(28.0, 700))?, p.measure(&of, ui(14.0, 400))?);
        let x0 = r - nw - ow;
        p.text(&num, ui(28.0, 700), rect(x0, 78.0, x0 + nw + 2.0, 114.0), Align::Left, &full)?;
        p.text(&of, ui(14.0, 400), rect(x0 + nw, 86.0, r + 2.0, 112.0), Align::Left, &soft)?;
        p.text(if n >= GLASSES { "أحسنت! اضغط للبدء" } else { "اضغط لكوب" }, ui(11.0, 400), rect(l - 10.0, 112.0, r, 128.0), Align::Right, &soft)
    }
}

/// 19: the processor and the memory with their last minute drawn, and the battery.
pub struct Pulse {
    cpu: Vec<f32>,
    ram: Vec<f32>,
    last: Option<(u64, u64)>,
}
const SAMPLES: usize = 30;
impl Pulse {
    pub fn new() -> Pulse {
        Pulse { cpu: vec![], ram: vec![], last: None }
    }
    fn sample(&mut self) {
        unsafe {
            let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
            if GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).is_ok() {
                let v = |f: FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
                let (i, total) = (v(idle), v(kernel) + v(user));
                if let Some((li, lt)) = self.last {
                    let (di, dt) = (i.saturating_sub(li), total.saturating_sub(lt));
                    if dt > 0 {
                        push(&mut self.cpu, 100.0 * (1.0 - di as f32 / dt as f32));
                    }
                }
                self.last = Some((i, total));
            }
            let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
            if GlobalMemoryStatusEx(&mut m).is_ok() {
                push(&mut self.ram, m.dwMemoryLoad as f32);
            }
        }
    }
}
impl Default for Pulse {
    fn default() -> Self {
        Pulse::new()
    }
}
fn push(v: &mut Vec<f32>, x: f32) {
    v.push(x.clamp(0.0, 100.0));
    if v.len() > SAMPLES {
        v.remove(0);
    }
}
impl Face for Pulse {
    fn size(&self) -> (f32, f32) {
        (370.0, 176.0)
    }
    fn id(&self) -> &str {
        "pulse"
    }
    fn tick(&self) -> u32 {
        2000
    }
    fn draw(&mut self, ctx: &Ctx) -> Result<()> {
        self.sample();
        let p = P::new(ctx);
        let (w, h) = self.size();
        p.card(w, h)?;
        let full: ID2D1Brush = p.solid(white(1.0))?.cast()?;
        let soft: ID2D1Brush = p.solid(white(0.62))?.cast()?;
        let (l, r, gap) = (18.0, w - 18.0, 12.0);
        let cw = (r - l - 2.0 * gap) / 3.0;
        let top = 30.0;
        let col = |i: usize| (r - (i as f32 + 1.0) * cw - i as f32 * gap, r - i as f32 * (cw + gap));

        for (i, (label, vals, color)) in [("المعالج", &self.cpu, rgba(143, 123, 255, 1.0)), ("الذاكرة", &self.ram, rgba(116, 192, 252, 1.0))].into_iter().enumerate() {
            let (x0, x1) = col(i);
            p.text(label, ui(11.0, 400), rect(x0, top, x1, top + 16.0), Align::Right, &soft)?;
            let v = vals.last().copied();
            p.text(&v.map(|v| format!("{}%", v.round() as i32)).unwrap_or("…".into()), ui(22.0, 600), rect(x0, top + 16.0, x1, top + 46.0), Align::Right, &full)?;
            // the last minute, newest on the left edge (as the text reads), filled under the line
            if vals.len() >= 2 {
                let (gx0, gx1, gy0, gy1) = (x1 - 100.0, x1, top + 52.0, top + 108.0);
                let n = SAMPLES.max(vals.len()) - 1;
                let pts: Vec<(f32, f32)> = vals.iter().enumerate().map(|(k, v)| {
                    let k = k + (SAMPLES - vals.len().min(SAMPLES));
                    (gx0 + (gx1 - gx0) * k as f32 / n as f32, gy1 - 4.0 - (gy1 - gy0 - 8.0) * v / 100.0)
                }).collect();
                let mut area = pts.clone();
                area.push((pts.last().unwrap().0, gy1));
                area.push((pts[0].0, gy1));
                let mut fill = color;
                fill.a = 0.18;
                p.poly(&area, &p.solid(fill)?.cast()?, true, 0.0)?;
                p.poly(&pts, &p.solid(color)?.cast()?, false, 2.0)?;
            }
        }

        // the battery
        let (x0, x1) = col(2);
        p.text("البطارية", ui(11.0, 400), rect(x0, top, x1, top + 16.0), Align::Right, &soft)?;
        let mut ps = SYSTEM_POWER_STATUS::default();
        let has = unsafe { GetSystemPowerStatus(&mut ps).is_ok() } && ps.BatteryFlag & 128 == 0 && ps.BatteryLifePercent <= 100;
        let (bx1, by0) = (x1, top + 60.0);
        let body = rect(bx1 - 78.0, by0, bx1, by0 + 34.0);
        p.stroke_round(rect(body.left + 1.0, body.top + 1.0, body.right - 1.0, body.bottom - 1.0), 7.0, &p.solid(white(0.5))?.cast()?, 2.0);
        p.fill_round(rect(body.left - 6.0, by0 + 12.0, body.left - 2.0, by0 + 22.0), 2.0, &p.solid(white(0.5))?.cast()?);
        if has {
            let pct = ps.BatteryLifePercent as f32;
            p.text(&format!("{}%", pct as i32), ui(22.0, 600), rect(x0, top + 16.0, x1, top + 46.0), Align::Right, &full)?;
            let inner = rect(body.left + 5.0, body.top + 5.0, body.right - 5.0, body.bottom - 5.0);
            let fw = (inner.right - inner.left) * pct / 100.0;
            let c = if pct <= 20.0 && ps.ACLineStatus != 1 { rgba(255, 93, 108, 1.0) } else { c3(GREEN, 1.0) };
            // fills from the right, as the bar reads
            p.fill_round(rect(inner.right - fw, inner.top, inner.right, inner.bottom), 4.0, &p.solid(c)?.cast()?);
        } else {
            p.text("موصول", ui(22.0, 600), rect(x0, top + 16.0, x1, top + 46.0), Align::Right, &full)?;
            p.text("بالكهرباء", ui(11.0, 400), body, Align::Center, &soft)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn todays_tasks() {
        let now = Now { y: 2026, mo: 11, d: 9, wd: 1, h: 13, mi: 20, s: 0, utc_ms: 0, offset_ms: 2 * 3_600_000 };
        let (start, _) = now.today_utc();
        let t = json!([
            { "title": "a", "tag": "urgent", "due": start + 15 * 3_600_000, "done": false },
            { "title": "late", "tag": "high", "due": start - 3_600_000, "done": false },
            { "title": "tomorrow", "due": start + 30 * 3_600_000, "done": false },
            { "title": "no date", "due": null, "done": false },
            { "title": "done", "done": true, "doneAt": start + 9 * 3_600_000 },
            { "title": "done before", "done": true, "doneAt": start - 9 * 3_600_000 }
        ]);
        let (rows, d, all) = today(&t, &now);
        let names: Vec<&str> = rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(names, ["late", "a", "done"]);
        assert_eq!((d, all), (1, 3));
        assert_eq!(rows[0].time, "متأخرة");
        assert_eq!(rows[1].time, crate::clock::hm(15, 0));
    }

    #[test]
    fn task_times_as_tasks_rs_writes_them() {
        // (1.8.7) f64 JSON numbers (1800000000000.0), not i64: they were all left out
        let now = Now { y: 2026, mo: 11, d: 9, wd: 1, h: 13, mi: 20, s: 0, utc_ms: 0, offset_ms: 0 };
        let (start, _) = now.today_utc();
        let t = json!([
            { "title": "a", "due": (start + 15 * 3_600_000) as f64, "done": false },
            { "title": "done", "done": true, "doneAt": (start + 9 * 3_600_000) as f64 }
        ]);
        let (rows, d, all) = today(&t, &now);
        assert_eq!((rows.len(), d, all), (2, 1, 2));
    }
}
