//! System status (system.js): CPU / RAM, clipboard history (memory only), weather, and the place
//! (a city picked in Settings, else the IP location, else the time zone's capital), shared with the
//! prayer times.
use std::sync::{atomic::{AtomicU64, Ordering}, Arc, Mutex};

use nova_core::http;
use serde_json::{json, Value};

use crate::ctx::{now_ms, Ctx, Wake};

const MAX_CLIPS: usize = 8;
// pinned clips (1.8.3): kept on top and on disk, never pushed out by new copies
const MAX_PINS: usize = 10;
const PINS_FILE: &str = "clips-pinned.json";
const WEATHER_FRESH: f64 = 3.0 * 60.0 * 60.0 * 1000.0;
const DAY: f64 = 24.0 * 60.0 * 60.0 * 1000.0;
// after a failure: 2, 5, then every 15 minutes
const RETRY_MIN: [u64; 3] = [2, 5, 15];

/// JS Math.round
fn round(v: f64) -> f64 {
    nova_core::json::js_round(v)
}

struct State {
    clips: Vec<String>,
    pins: Vec<String>,
    last_clip: String,
    smart: Option<Value>,
    weather: Option<Value>,
    manual: Option<Value>,
    location: Option<Value>,
    located_at: f64,
    weather_fails: usize,
}

pub struct System {
    ctx: Ctx,
    st: Mutex<State>,
    stats_every: AtomicU64,
    weather_wake: Wake,
    stats_wake: Wake,
    on_location: Mutex<Vec<Box<dyn Fn(Value) + Send>>>,
}

// First start with no internet and no location saved yet: the capital of the PC's time zone, so
// prayer times show at once. Marked approximate, never saved.
const CAPITALS: &[(&str, &str, f64, f64, &str)] = &[
    ("Africa/Cairo", "Cairo", 30.0444, 31.2357, "EG"), ("Asia/Riyadh", "Riyadh", 24.7136, 46.6753, "SA"),
    ("Asia/Dubai", "Dubai", 25.2048, 55.2708, "AE"), ("Asia/Kuwait", "Kuwait City", 29.3759, 47.9774, "KW"),
    ("Asia/Qatar", "Doha", 25.2854, 51.531, "QA"), ("Asia/Bahrain", "Manama", 26.2285, 50.586, "BH"),
    ("Asia/Muscat", "Muscat", 23.588, 58.3829, "OM"), ("Asia/Amman", "Amman", 31.9539, 35.9106, "JO"),
    ("Asia/Beirut", "Beirut", 33.8938, 35.5018, "LB"), ("Asia/Damascus", "Damascus", 33.5138, 36.2765, "SY"),
    ("Asia/Baghdad", "Baghdad", 33.3152, 44.3661, "IQ"), ("Asia/Aden", "Sana'a", 15.3694, 44.191, "YE"),
    ("Asia/Gaza", "Gaza", 31.5017, 34.4668, "PS"), ("Asia/Hebron", "Hebron", 31.5326, 35.0998, "PS"),
    ("Africa/Casablanca", "Rabat", 34.0209, -6.8416, "MA"), ("Africa/Algiers", "Algiers", 36.7538, 3.0588, "DZ"),
    ("Africa/Tunis", "Tunis", 36.8065, 10.1815, "TN"), ("Africa/Tripoli", "Tripoli", 32.8872, 13.1913, "LY"),
    ("Africa/Khartoum", "Khartoum", 15.5007, 32.5599, "SD"), ("Europe/Istanbul", "Istanbul", 41.0082, 28.9784, "TR"),
    ("Asia/Karachi", "Karachi", 24.8607, 67.0011, "PK"), ("Asia/Jakarta", "Jakarta", -6.2088, 106.8456, "ID"),
    ("Asia/Kuala_Lumpur", "Kuala Lumpur", 3.139, 101.6869, "MY"), ("Europe/London", "London", 51.5072, -0.1276, "GB"),
    ("Europe/Paris", "Paris", 48.8566, 2.3522, "FR"), ("America/New_York", "New York", 40.7128, -74.006, "US"),
];

fn capital_of_zone() -> Option<Value> {
    let zone = nova_prayer::icu::default_zone();
    CAPITALS.iter().find(|c| c.0 == zone).map(|c| json!({ "city": c.1, "lat": c.2, "lon": c.3, "country": c.4, "timezone": zone, "approx": true }))
}

/// IP geolocation with fallbacks (free services rate-limit often).
fn locate() -> Result<Value, String> {
    type Pick = fn(&Value) -> Option<Value>;
    let num = |v: &Value| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()));
    let services: [(&str, Pick); 3] = [
        ("https://ipwho.is/", |j| (j["success"] != false).then(|| json!({ "lat": j["latitude"], "lon": j["longitude"], "city": j["city"], "country": j["country_code"], "timezone": j["timezone"]["id"] }))),
        ("https://get.geojs.io/v1/ip/geo.json", |j| Some(json!({ "lat": j["latitude"].as_str().and_then(|s| s.parse::<f64>().ok()).map(Value::from).unwrap_or(j["latitude"].clone()), "lon": j["longitude"].as_str().and_then(|s| s.parse::<f64>().ok()).map(Value::from).unwrap_or(j["longitude"].clone()), "city": j["city"], "country": j["country_code"], "timezone": j["timezone"] }))),
        ("https://ipapi.co/json/", |j| Some(json!({ "lat": j["latitude"], "lon": j["longitude"], "city": j["city"], "country": j["country_code"], "timezone": j["timezone"] }))),
    ];
    for (url, pick) in services {
        if let Ok(j) = http::get_json(url, 10000) {
            if let Some(loc) = pick(&j) {
                if num(&loc["lat"]).is_some_and(f64::is_finite) && num(&loc["lon"]).is_some_and(f64::is_finite) {
                    // drop missing fields, like JSON.stringify drops undefined
                    let o: serde_json::Map<String, Value> = loc.as_object().unwrap().iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.clone(), v.clone())).collect();
                    return Ok(Value::Object(o));
                }
            }
        }
    }
    Err("no location".into())
}

impl System {
    pub fn new(ctx: Ctx) -> Arc<System> {
        let store = ctx.store();
        let weather = store.load("weather-cache.json").filter(|s| now_ms() - s["at"].as_f64().unwrap_or(0.0) < WEATHER_FRESH).map(|s| s["weather"].clone());
        let pins = store.load(PINS_FILE).and_then(|v| v.as_array().map(|a| a.iter().filter_map(|t| t.as_str().map(str::to_string)).take(MAX_PINS).collect())).unwrap_or_default();
        let st = State {
            clips: Vec::new(),
            pins,
            last_clip: nova_sys::clip::read_text(),
            smart: None,
            weather,
            manual: store.load("location.json").filter(|v| !v.is_null()),
            location: store.load("location-cache.json").filter(|v| !v.is_null()),
            located_at: 0.0,
            weather_fails: 0,
        };
        Arc::new(System { ctx, st: Mutex::new(st), stats_every: AtomicU64::new(2000), weather_wake: Wake::new(), stats_wake: Wake::new(), on_location: Mutex::new(Vec::new()) })
    }

    pub fn start(self: &Arc<Self>) {
        let me = self.clone();
        std::thread::spawn(move || me.stats_loop());
        if let Some(w) = self.st.lock().unwrap().weather.clone() {
            self.ctx.send("weather", w); // the saved one, until a fresh one arrives
        }
        let me = self.clone();
        std::thread::spawn(move || me.weather_loop());
    }

    // ---- CPU / RAM ----
    fn stats_loop(&self) {
        let mut last = nova_sys::sysinfo::cpu_times();
        let mut net = (crate::netspeed::counters(), std::time::Instant::now());
        loop {
            self.stats_wake.sleep(self.stats_every.load(Ordering::Relaxed));
            // (1.8.7) bytes a second down and up since the last reading
            let now_net = (crate::netspeed::counters(), std::time::Instant::now());
            let secs = now_net.1.duration_since(net.1).as_secs_f64().max(0.001);
            let rx = (now_net.0 .0.saturating_sub(net.0 .0) as f64 / secs).round();
            let tx = (now_net.0 .1.saturating_sub(net.0 .1) as f64 / secs).round();
            net = now_net;
            let ping = crate::netspeed::PING_MS.load(Ordering::Relaxed);
            let now = nova_sys::sysinfo::cpu_times();
            let d_total = now.1.saturating_sub(last.1);
            let cpu = if d_total > 0 { 1.0 - now.0.saturating_sub(last.0) as f64 / d_total as f64 } else { 0.0 };
            last = now;
            let (total, free) = nova_sys::sysinfo::memory();
            let used = total - free;
            self.ctx.send("stats", json!({ "cpu": cpu, "ram": used / total, "ramUsed": used, "ramTotal": total, "rx": rx, "tx": tx, "ping": if ping < 0 { Value::Null } else { json!(ping) } }));
        }
    }

    /// CPU / RAM every 2 s, every 5 s with the performance options (or the battery saver) on
    pub fn set_stats_every(&self, ms: u64) {
        if self.stats_every.swap(ms, Ordering::Relaxed) != ms {
            self.stats_wake.wake();
        }
    }

    // ---- clipboard (kept in memory only; may contain sensitive text) ----
    /// A new copy (the bridge's clipboard counter moved). Private copies are never read.
    pub fn clipboard_changed(&self, is_private: bool) {
        if is_private {
            return;
        }
        let text = nova_sys::clip::read_text();
        let mut st = self.st.lock().unwrap();
        if text.is_empty() || text == st.last_clip {
            return;
        }
        st.last_clip = text.clone();
        if text.trim_matches(crate::smartclip::js_space).is_empty() {
            return;
        }
        if self.ctx.on("smartClipboard") {
            st.smart = crate::smartclip::classify(&text).ok().flatten();
            if let Some(s) = st.smart.clone() {
                self.ctx.send("clip-smart", s);
            }
        }
        if !self.ctx.on("clipboardHistory") {
            return;
        }
        if st.pins.contains(&text) {
            return; // already on top, pinned
        }
        st.clips.retain(|c| *c != text);
        st.clips.insert(0, text);
        st.clips.truncate(MAX_CLIPS);
        let list = Self::listing(&st);
        drop(st);
        self.ctx.send("clipboard", list);
    }

    /// The list the page shows: the pinned clips first, then the recent ones.
    fn entries(st: &State) -> Vec<(String, bool)> {
        st.pins.iter().map(|p| (p.clone(), true)).chain(st.clips.iter().filter(|c| !st.pins.contains(c)).map(|c| (c.clone(), false))).collect()
    }

    /// Each clip as the page gets it: the first 300 characters (UTF-16 units, as c.slice(0, 300)),
    /// whether it's pinned, and whether there's more to see in the full view.
    fn listing(st: &State) -> Value {
        Value::Array(
            Self::entries(st)
                .into_iter()
                .map(|(c, pinned)| {
                    let u: Vec<u16> = c.encode_utf16().collect();
                    let short = String::from_utf16_lossy(&u[..u.len().min(300)]);
                    let long = u.len() > 300 || c.contains('\n');
                    json!({ "text": short, "pinned": pinned, "long": long, "chars": c.chars().count() })
                })
                .collect(),
        )
    }

    /// The whole text of clip `i` (the full view).
    pub fn clip_full(&self, i: usize) -> Value {
        let st = self.st.lock().unwrap();
        Self::entries(&st).get(i).map(|(c, _)| Value::String(c.clone())).unwrap_or(Value::Null)
    }

    /// Pins or unpins clip `i`. Pins are saved on this PC (the user chose to keep them).
    pub fn pin_clip(&self, i: usize, on: bool) {
        let mut st = self.st.lock().unwrap();
        let Some((text, pinned)) = Self::entries(&st).get(i).cloned() else { return };
        if on && !pinned {
            st.pins.insert(0, text.clone());
            st.pins.truncate(MAX_PINS);
            st.clips.retain(|c| *c != text);
        } else if !on && pinned {
            st.pins.retain(|c| *c != text);
            st.clips.retain(|c| *c != text);
            st.clips.insert(0, text);
            st.clips.truncate(MAX_CLIPS);
        } else {
            return;
        }
        let _ = self.ctx.store().save(PINS_FILE, &json!(st.pins));
        let list = Self::listing(&st);
        drop(st);
        self.ctx.send("clipboard", list);
    }

    /// What the last copy was recognised as (acted on by the app, never on data from the page).
    pub fn last_smart(&self) -> Option<Value> {
        self.st.lock().unwrap().smart.clone()
    }

    /// Writing to the clipboard ourselves must not be announced again.
    pub fn write_quiet(&self, text: &str) {
        self.st.lock().unwrap().last_clip = text.to_string();
        nova_sys::clip::write_text(text);
    }

    pub fn copy_clip(&self, i: usize) {
        let mut st = self.st.lock().unwrap();
        let Some((text, pinned)) = Self::entries(&st).get(i).cloned() else { return };
        st.last_clip = text.clone();
        nova_sys::clip::write_text(&text);
        if !pinned {
            st.clips.retain(|c| *c != text);
            st.clips.insert(0, text);
        }
        let list = Self::listing(&st);
        drop(st);
        self.ctx.send("clipboard", list);
    }

    /// Clears the history; pinned clips stay until they're unpinned.
    pub fn clear_clips(&self) {
        let mut st = self.st.lock().unwrap();
        st.clips.clear();
        let list = Self::listing(&st);
        drop(st);
        self.ctx.send("clipboard", list);
    }

    pub fn snapshot(&self) -> Value {
        let st = self.st.lock().unwrap();
        json!({ "clips": Self::listing(&st), "weather": st.weather.clone().unwrap_or(Value::Null) })
    }

    // ---- the place ----
    /// A city picked in Settings wins; otherwise the IP location, looked up again once a day,
    /// falling back to the last one found (or the time zone's capital).
    pub fn get_location(&self) -> Result<Value, String> {
        let st = self.st.lock().unwrap();
        if let Some(m) = st.manual.clone() {
            return Ok(json!({ "lat": m["lat"], "lon": m["lon"], "city": m["name"], "country": m["countryCode"].as_str().unwrap_or(""), "timezone": m["timezone"] }));
        }
        let stale = st.location.as_ref().is_none_or(|l| l["approx"] == true) || now_ms() - st.located_at > DAY;
        drop(st);
        if stale {
            match locate() {
                Ok(loc) => {
                    let mut st = self.st.lock().unwrap();
                    st.location = Some(loc.clone());
                    st.located_at = now_ms();
                    let _ = self.ctx.store().save("location-cache.json", &loc);
                }
                Err(e) => {
                    let mut st = self.st.lock().unwrap();
                    if st.location.is_none() {
                        st.location = capital_of_zone();
                    }
                    if st.location.is_none() {
                        return Err(e);
                    }
                    st.located_at = now_ms() - DAY + 30.0 * 60.0 * 1000.0; // try again in 30 minutes
                }
            }
        }
        Ok(self.st.lock().unwrap().location.clone().unwrap())
    }

    /// After sleep or a lost connection: weather (and a guessed location) right away.
    pub fn refresh_now(&self) {
        let mut st = self.st.lock().unwrap();
        if st.location.as_ref().is_some_and(|l| l["approx"] == true) {
            st.located_at = 0.0;
        }
        st.weather_fails = 0;
        drop(st);
        self.weather_wake.wake();
    }

    pub fn location_info(&self) -> Value {
        let st = self.st.lock().unwrap();
        match &st.manual {
            Some(m) => {
                let mut v = json!({ "mode": "manual" });
                for (k, x) in m.as_object().unwrap() {
                    v[k] = x.clone();
                }
                v
            }
            None => json!({ "mode": "auto", "name": st.location.as_ref().and_then(|l| l["city"].as_str()).unwrap_or("") }),
        }
    }

    /// City search for Settings (Open-Meteo geocoding, no key). None when the search failed.
    pub fn search_city(&self, query: &str) -> Option<Value> {
        let q: String = query.trim().chars().take(80).collect();
        if q.chars().count() < 2 {
            return Some(json!([]));
        }
        let url = format!("https://geocoding-api.open-meteo.com/v1/search?count=6&language=ar&format=json&name={}", http::encode_component(&q));
        let j = http::get_json(&url, 10000).ok()?;
        let list: Vec<Value> = j["results"].as_array().cloned().unwrap_or_default().iter().map(|r| json!({
            "name": r["name"], "country": r["country"].as_str().unwrap_or(""), "countryCode": r["country_code"].as_str().unwrap_or(""),
            "admin": r["admin1"].as_str().unwrap_or(""), "lat": r["latitude"], "lon": r["longitude"], "timezone": r["timezone"].as_str().unwrap_or(""),
        })).collect();
        Some(Value::Array(list))
    }

    /// null = back to automatic
    pub fn set_location(&self, loc: &Value) {
        let ok = loc["lat"].as_f64().is_some_and(f64::is_finite) && loc["lon"].as_f64().is_some_and(f64::is_finite) && loc["name"].is_string();
        let cut = |v: &Value, n: usize| -> String { v.as_str().map(|s| s.chars().take(n).collect()).unwrap_or_default() };
        let manual = ok.then(|| {
            let cc = loc["countryCode"].as_str().filter(|c| c.len() == 2 && c.bytes().all(|b| b.is_ascii_alphabetic())).map(|c| c.to_uppercase()).unwrap_or_default();
            json!({ "name": cut(&loc["name"], 80), "country": cut(&loc["country"], 80), "countryCode": cc, "lat": loc["lat"], "lon": loc["lon"], "timezone": cut(&loc["timezone"], 60) })
        });
        let _ = self.ctx.store().save("location.json", manual.as_ref().unwrap_or(&Value::Null));
        {
            let mut st = self.st.lock().unwrap();
            st.manual = manual;
            st.location = None;
        }
        self.weather_wake.wake();
        let info = self.location_info();
        for f in self.on_location.lock().unwrap().iter() {
            f(info.clone());
        }
    }

    pub fn on_location_change(&self, f: impl Fn(Value) + Send + 'static) {
        self.on_location.lock().unwrap().push(Box::new(f));
    }

    // ---- weather (Open-Meteo, location from IP) ----
    fn fetch_weather(&self) -> Result<(), String> {
        let loc = self.get_location()?;
        let url = format!(
            "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,weather_code,is_day&daily=temperature_2m_max,temperature_2m_min&timezone=auto&forecast_days=1",
            nova_core::json::number(loc["lat"].as_f64().unwrap_or(0.0)),
            nova_core::json::number(loc["lon"].as_f64().unwrap_or(0.0))
        );
        let w = http::get_json(&url, 10000)?;
        let c = &w["current"];
        let weather = json!({
            "city": loc["city"].as_str().unwrap_or(""),
            "temp": round(c["temperature_2m"].as_f64().ok_or("bad weather")?),
            "code": c["weather_code"],
            "isDay": c["is_day"].as_f64().unwrap_or(0.0) != 0.0,
            "hi": round(w["daily"]["temperature_2m_max"][0].as_f64().unwrap_or(0.0)),
            "lo": round(w["daily"]["temperature_2m_min"][0].as_f64().unwrap_or(0.0)),
        });
        let _ = self.ctx.store().save("weather-cache.json", &json!({ "at": now_ms(), "weather": weather }));
        self.st.lock().unwrap().weather = Some(weather.clone());
        self.ctx.send("weather", weather);
        Ok(())
    }

    fn weather_loop(&self) {
        loop {
            let wait = match self.fetch_weather() {
                Ok(()) => {
                    let mut st = self.st.lock().unwrap();
                    if st.weather_fails > 0 {
                        nova_core::log::warn("[weather] back");
                    }
                    st.weather_fails = 0;
                    20 * 60 * 1000
                }
                Err(e) => {
                    let mut st = self.st.lock().unwrap();
                    if st.weather_fails == 0 {
                        nova_core::log::warn(&format!("[weather] {e} (retrying quietly)"));
                    }
                    let m = RETRY_MIN[st.weather_fails.min(RETRY_MIN.len() - 1)];
                    st.weather_fails += 1;
                    m * 60 * 1000
                }
            };
            self.weather_wake.sleep(wait);
        }
    }
}

