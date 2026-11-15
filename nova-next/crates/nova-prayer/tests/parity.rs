// nova-prayer against the current prayer.js (parity/oracle/gen-prayer.cjs): every day of every place
// must give the same minutes, method, Hijri date and the same failures; the Hijri date of every
// day 1900-2076 must match.
use std::{fs, path::PathBuf};

use nova_prayer::{calc_day, hijri_of, Day, Settings};
use serde_json::Value;

fn fixture(name: &str) -> Value {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../parity/fixtures").join(name);
    serde_json::from_str(&fs::read_to_string(p).expect("run parity/oracle/gen-prayer.cjs first")).unwrap()
}

fn day(v: &Value) -> Day {
    Day { y: v["y"].as_i64().unwrap() as i32, m: v["m"].as_u64().unwrap() as u32, d: v["d"].as_u64().unwrap() as u32 }
}

/// Days where 1.8 knowingly differs from 1.7, both about time zone data, not the prayer maths:
///   Casablanca 2026+  Windows' zone data (the PC's own clock) puts Morocco on UTC+0 outside summer
///                     from 2026; Chromium's tz 2025b keeps UTC+1. 1.8 follows the Windows clock so
///                     the times agree with the taskbar.
///   Tehran 1978       a historical change between tz 2022g (Windows ICU) and 2025b
fn known_difference(city: &str, year: i64) -> bool {
    (city == "Casablanca" && year >= 2026) || (city == "Tehran" && year == 1978)
}

#[test]
fn prayer_times_like_prayer_js() {
    let cases = fixture("prayer.json");
    let mut wrong = Vec::new();
    let cases = cases.as_array().unwrap();
    for c in cases {
        let loc = &c["loc"];
        let s = Settings { asr_hanafi: c["hanafi"].as_bool().unwrap(), hijri_adjust: c["adjust"].as_i64().unwrap() };
        let tz = loc["timezone"].as_str().unwrap();
        let got = calc_day(day(&c["day"]), loc["country"].as_str().unwrap(), loc["lat"].as_f64().unwrap(), loc["lon"].as_f64().unwrap(), tz, tz, &s);
        let ok = match (&got, c.get("out"), c.get("error")) {
            (Ok(v), Some(want), _) => v == want,
            (Err(e), _, Some(want)) => e == want.as_str().unwrap(),
            _ => false,
        };
        if !ok && !known_difference(loc["city"].as_str().unwrap(), c["day"]["y"].as_i64().unwrap()) {
            wrong.push(format!("DIFF	{}	{}	{}	{}", loc["city"].as_str().unwrap(), c["day"]["y"], match &got { Ok(v) => v["times"].to_string(), Err(e) => e.clone() }, c.get("out").map(|o| o["times"].to_string()).or(c.get("error").map(|e| e.to_string())).unwrap()));
        }
    }
    assert!(wrong.is_empty(), "{} of {} days differ:\n{}", wrong.len(), cases.len(), wrong.iter().take(15).cloned().collect::<Vec<_>>().join("\n"));
}

#[test]
fn hijri_like_prayer_js() {
    let all = fixture("hijri.json");
    let all = all.as_array().unwrap();
    let mut wrong = Vec::new();
    for h in all {
        let d = Day { y: h[0].as_i64().unwrap() as i32, m: h[1].as_u64().unwrap() as u32, d: h[2].as_u64().unwrap() as u32 };
        let (text, month) = hijri_of(d, h[3].as_i64().unwrap());
        if text != h[4].as_str().unwrap() || month as u64 != h[5].as_u64().unwrap() {
            wrong.push(format!("{d:?} adjust {}: got {text} ({month}) want {} ({})", h[3], h[4], h[5]));
        }
    }
    assert!(wrong.is_empty(), "{} of {} dates differ:\n{}", wrong.len(), all.len(), wrong.iter().take(15).cloned().collect::<Vec<_>>().join("\n"));
}
