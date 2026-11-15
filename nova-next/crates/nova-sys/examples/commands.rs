// Step 3's command check: with a media session playing (run scratchpad smtc-play.ps1 first, or any
// player), send nova-sys the island's commands and confirm each takes effect in the next lines.
// Volume is only "set" to its current value, so the PC's sound doesn't change.
//   cargo run --release -p nova-sys --example commands
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde_json::Value;

fn main() {
    let last: Arc<Mutex<Option<Value>>> = Arc::default();
    let l2 = last.clone();
    let keep = move |line: String| {
        if let Ok(v) = serde_json::from_str::<Value>(line.trim_start_matches('\u{feff}')) {
            if v.get("has").is_some() {
                *l2.lock().unwrap() = Some(v);
            }
        }
    };
    // "cs": the same checks against the C# helper (what 1.7 runs), through its stdin
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    if std::env::args().nth(1).as_deref() == Some("cs") {
        use std::io::{BufRead, Write};
        let helper = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../helper/nova-helper.exe");
        let mut child = std::process::Command::new(helper).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().unwrap();
        let out = child.stdout.take().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
                keep(line);
            }
        });
        std::thread::spawn(move || {
            for c in rx {
                let _ = writeln!(stdin, "{c}");
            }
        });
    } else {
        let bridge = nova_sys::start(keep);
        std::thread::spawn(move || {
            for c in rx {
                let _ = bridge.send(c);
            }
        });
    }
    // waits up to 3 s for a line that satisfies `ok`
    let until = |what: &str, ok: &dyn Fn(&Value) -> bool| {
        let end = Instant::now() + Duration::from_secs(3);
        while Instant::now() < end {
            if let Some(v) = last.lock().unwrap().clone() {
                if ok(&v) {
                    println!("  ok    {what}");
                    return true;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        println!("  FAIL  {what}: last line {}", last.lock().unwrap().clone().unwrap_or_default());
        false
    };
    if !until("a media session is playing", &|v| v["has"] == true && v["playing"] == true) {
        return;
    }
    let v = last.lock().unwrap().clone().unwrap();
    let app = v["app"].as_str().unwrap_or("").to_string();
    println!("  session: {:?} by {:?} ({app})", v["title"], v["artist"]);
    let send = |c: &str| tx.send(c.to_string()).unwrap();
    let mut pass = 0;
    let mut n = 0;
    let mut check = |ok: bool| {
        n += 1;
        pass += ok as i32;
    };
    send("toggle");
    check(until("toggle pauses", &|v| v["playing"] == false));
    send("toggle");
    check(until("toggle plays again", &|v| v["playing"] == true));
    send("seek 1");
    check(until("seek 1 moves to ~1 s", &|v| v["pos"].as_f64().is_some_and(|p| p < 1.8)));
    send(&format!("pauseapp {app}"));
    check(until("pauseapp pauses that app", &|v| v["playing"] == false));
    send(&format!("seekapp {app} 2"));
    check(until("seekapp moves that app to 2 s", &|v| v["pos"].as_f64().is_some_and(|p| (p - 2.0).abs() < 0.6)));
    send(&format!("playapp {app}"));
    check(until("playapp plays that app", &|v| v["playing"] == true));
    let vol = v["vol"].as_f64().unwrap_or(0.5);
    send(&format!("vol {vol}"));
    check(until("vol to the same value keeps it", &|v| (v["vol"].as_f64().unwrap_or(-1.0) - vol).abs() < 0.002));
    println!("{pass}/{n} commands took effect");
}
