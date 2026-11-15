// Step 3's check: the C# helper (helper/nova-helper.exe, what 1.7 runs) and nova-sys side by side.
// Every 500 ms the latest line of each is compared field by field.
//   cargo run --release -p nova-sys --example diff -- <seconds>
// Allowed: pos within 0.6 s (the two read the clock at slightly different moments), idle within
// 1 s. The thumbnail is compared as "the last one sent". Fields that differ only right after a
// change (the two sample at different moments) are reported as "transient" when they agree again
// on the next sample.
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde_json::Value;

type Latest = Arc<Mutex<(Option<Value>, String)>>; // last line, last thumb

fn keep(latest: &Latest, line: &str) {
    let Ok(v) = serde_json::from_str::<Value>(line) else { return };
    if v.get("has").is_none() {
        return;
    }
    let mut l = latest.lock().unwrap();
    if let Some(t) = v.get("thumb").and_then(Value::as_str) {
        l.1 = t.to_string();
    }
    l.0 = Some(v);
}

fn same(k: &str, a: &Value, b: &Value) -> bool {
    match k {
        "pos" => (a.as_f64().unwrap_or(0.0) - b.as_f64().unwrap_or(0.0)).abs() <= 0.6,
        "idle" => (a.as_i64().unwrap_or(0) - b.as_i64().unwrap_or(0)).abs() <= 1,
        _ => a == b,
    }
}

fn main() {
    let secs: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(60);
    let helper = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../helper/nova-helper.exe");
    let cs: Latest = Arc::default();
    let rs: Latest = Arc::default();

    let mut child = Command::new(&helper).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().expect("helper/nova-helper.exe");
    let out = child.stdout.take().unwrap();
    let cs2 = cs.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            keep(&cs2, line.trim_start_matches('\u{feff}'));
        }
    });
    let rs2 = rs.clone();
    let _cmd = nova_sys::start(move |line| keep(&rs2, &line));

    std::thread::sleep(Duration::from_secs(3));
    let end = Instant::now() + Duration::from_secs(secs);
    let mut samples = 0;
    let mut diffs: BTreeMap<String, (u32, u32, String)> = BTreeMap::new(); // field -> (differ, transient, example)
    let mut prev_bad: BTreeMap<String, bool> = BTreeMap::new();
    while Instant::now() < end {
        std::thread::sleep(Duration::from_millis(500));
        let (a, at) = cs.lock().unwrap().clone();
        let (b, bt) = rs.lock().unwrap().clone();
        let (Some(mut a), Some(mut b)) = (a, b) else { continue };
        a["thumb"] = Value::String(at.len().to_string() + ":" + &at.chars().take(24).collect::<String>());
        b["thumb"] = Value::String(bt.len().to_string() + ":" + &bt.chars().take(24).collect::<String>());
        samples += 1;
        let keys: Vec<String> = a.as_object().unwrap().keys().chain(b.as_object().unwrap().keys()).cloned().collect();
        let mut seen = std::collections::BTreeSet::new();
        for k in keys {
            if !seen.insert(k.clone()) {
                continue;
            }
            let (x, y) = (a.get(&k).unwrap_or(&Value::Null), b.get(&k).unwrap_or(&Value::Null));
            let bad = !same(&k, x, y);
            let e = diffs.entry(k.clone()).or_insert((0, 0, String::new()));
            if bad {
                e.0 += 1;
                if e.2.is_empty() {
                    e.2 = format!("C# {x}  |  Rust {y}");
                }
            } else if prev_bad.get(&k).copied().unwrap_or(false) {
                e.1 += 1;
            }
            prev_bad.insert(k, bad);
        }
    }
    let _ = child.kill();
    println!("{samples} samples over {secs} s");
    for (k, (n, transient, ex)) in &diffs {
        if *n > 0 {
            println!("  {k:9} differs in {n} samples ({transient} recovered next sample)  e.g. {ex}");
        }
    }
    let fields = diffs.len();
    let clean = diffs.values().filter(|d| d.0 == 0).count();
    println!("{clean}/{fields} fields identical in every sample");
}
