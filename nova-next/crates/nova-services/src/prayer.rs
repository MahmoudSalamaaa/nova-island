//! The prayer-times service (prayer.js refresh / retry / snapshot): today and tomorrow for the
//! place, saved to prayer-cache.json, recalculated just after local midnight and when the place or
//! the settings change; after a failure (no place known yet) every 5 minutes, logged once.
use std::sync::{Arc, Mutex};

use nova_prayer::{calc_day, day_in, next_day, Settings};
use serde_json::{json, Value};

use crate::{
    ctx::{now_ms, Ctx, Wake},
    system::System,
};

pub struct Prayer {
    ctx: Ctx,
    system: Arc<System>,
    data: Mutex<Value>,
    failed: Mutex<bool>,
    approx: Mutex<bool>,
    wake: Wake,
}

impl Prayer {
    pub fn new(ctx: Ctx, system: Arc<System>) -> Arc<Prayer> {
        let data = ctx.store().load("prayer-cache.json").unwrap_or(Value::Null);
        Arc::new(Prayer { ctx, system, data: Mutex::new(data), failed: Mutex::new(false), approx: Mutex::new(false), wake: Wake::new() })
    }

    pub fn start(self: &Arc<Self>) {
        let me = self.clone();
        std::thread::spawn(move || loop {
            let wait = me.run();
            me.wake.sleep(wait);
        });
    }

    pub fn snapshot(&self) -> Value {
        self.data.lock().unwrap().clone()
    }

    /// Recalculate now (place or settings changed).
    pub fn refresh(&self) {
        self.wake.wake();
    }

    /// After sleep or a lost connection: recalculate if the last run failed or was approximate.
    pub fn retry(&self) {
        if *self.failed.lock().unwrap() || *self.approx.lock().unwrap() {
            self.wake.wake();
        }
    }

    /// One calculation; returns ms until the next one.
    fn run(&self) -> u64 {
        match self.calc() {
            Ok(data) => {
                let _ = self.ctx.store().save("prayer-cache.json", &data);
                *self.data.lock().unwrap() = data.clone();
                let zone = data["today"]["tz"].as_str().filter(|z| !z.is_empty()).map(str::to_string).unwrap_or_else(nova_prayer::icu::default_zone);
                self.ctx.send("prayer", data);
                let mut failed = self.failed.lock().unwrap();
                if *failed {
                    nova_core::log::warn("[prayer] back");
                }
                *failed = false;
                // next run just after midnight where the place is (1.8.7: it was the PC's, so a
                // city hours ahead of the PC kept yesterday's times until the PC's midnight)
                let f = nova_prayer::wall(&zone, now_ms()).map(|f| (f.hour * 3600 + f.minute * 60 + f.second) as u64).unwrap_or(0);
                (24 * 3600 + 60 - f.min(24 * 3600)) * 1000
            }
            Err(e) => {
                let mut failed = self.failed.lock().unwrap();
                if !*failed {
                    nova_core::log::warn(&format!("[prayer] {e} (retrying quietly)"));
                }
                *failed = true;
                5 * 60 * 1000
            }
        }
    }

    fn calc(&self) -> Result<Value, String> {
        let loc = self.system.get_location()?;
        let zone = loc["timezone"].as_str().filter(|z| !z.is_empty()).map(str::to_string).unwrap_or_else(nova_prayer::icu::default_zone);
        let today = day_in(&zone, now_ms()).ok_or("bad time zone")?;
        let tomorrow = next_day(today);
        let approx = loc["approx"] == true;
        *self.approx.lock().unwrap() = approx;
        let s = Settings { asr_hanafi: self.ctx.on("asrHanafi"), hijri_adjust: self.ctx.feature("hijriAdjust").as_i64().unwrap_or(0) };
        let (country, lat, lon) = (loc["country"].as_str().unwrap_or(""), loc["lat"].as_f64().unwrap_or(0.0), loc["lon"].as_f64().unwrap_or(0.0));
        let tz = loc["timezone"].as_str().unwrap_or("");
        let day = |d: nova_prayer::Day| -> Result<Value, String> {
            let mut v = calc_day(d, country, lat, lon, tz, &zone, &s)?;
            // (1.8.7) the place's minutes ahead of UTC that day, for the desktop widgets: they
            // have no time-zone data, and showed the PC's day and countdown for another city
            if let Some(o) = utc_offset_min(&zone, d) {
                v["utcOffset"] = json!(o);
            }
            Ok(v)
        };
        Ok(json!({
            "city": format!("{}{}", loc["city"].as_str().unwrap_or(""), if approx { " (approx.)" } else { "" }),
            "today": day(today)?,
            "tomorrow": day(tomorrow)?,
        }))
    }
}

/// Minutes `zone` is ahead of UTC at noon on `day`.
fn utc_offset_min(zone: &str, day: nova_prayer::Day) -> Option<i64> {
    let noon = nova_prayer::days_from_civil(day.y as i64, day.m as i64, day.d as i64) * 86_400_000 + 12 * 3_600_000;
    let f = nova_prayer::wall(zone, noon as f64)?;
    let wall = nova_prayer::days_from_civil(f.year as i64, f.month as i64, f.day as i64) * 86_400_000 + ((f.hour * 3600 + f.minute * 60 + f.second) as i64) * 1000;
    Some((wall - noon) / 60_000)
}

#[cfg(test)]
mod tests {
    #[test]
    fn offsets_of_places() {
        let day = nova_prayer::Day { y: 2026, m: 11, d: 13 };
        assert_eq!(super::utc_offset_min("Asia/Tokyo", day), Some(540));
        assert_eq!(super::utc_offset_min("Africa/Cairo", day), Some(120));
        assert_eq!(super::utc_offset_min("America/Los_Angeles", day), Some(-480));
        assert_eq!(super::utc_offset_min("America/Los_Angeles", nova_prayer::Day { y: 2026, m: 7, d: 1 }), Some(-420));
    }
}
