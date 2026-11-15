// Step 3's Apps check: "which of these apps have a window open" for every app in the user's
// apps-cache.json, from the C# helper and from nova-sys, must be the same list. Only reads: no window
// is focused (focusapp is only tried with an app that has no window, so the answer is "none").
//   cargo run --release -p nova-sys --example apps
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::mpsc,
    time::Duration,
};

use serde_json::Value;

fn main() {
    let cache = std::path::PathBuf::from(std::env::var("APPDATA").unwrap()).join("dynamic-nova/apps-cache.json");
    let apps: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(cache).expect("apps-cache.json")).unwrap();
    // apps.js: the exe hint is the icon path when it's an exe file
    let pairs: Vec<(String, String)> = apps
        .iter()
        .map(|a| {
            let icon = a["icon"].as_str().unwrap_or("");
            let exe = if a["kind"] == "file" && icon.to_lowercase().ends_with(".exe") { icon } else { "" };
            (a["id"].as_str().unwrap_or("").to_string(), exe.to_string())
        })
        .collect();
    let cmd = format!("runningapps\t{}", pairs.iter().map(|(i, e)| format!("{i}\t{e}")).collect::<Vec<_>>().join("\t"));
    let focus_none = "focusapp\tNo.Such.App_0000\t";

    // C# helper
    let helper = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../helper/nova-helper.exe");
    let mut child = Command::new(helper).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let (ctx, crx) = mpsc::channel::<Value>();
    let out = child.stdout.take().unwrap();
    std::thread::spawn(move || {
        for l in BufReader::new(out).lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str::<Value>(l.trim_start_matches('\u{feff}')) {
                if v.get("running").is_some() || v.get("focusapp").is_some() {
                    let _ = ctx.send(v);
                }
            }
        }
    });
    writeln!(stdin, "{cmd}").unwrap();
    writeln!(stdin, "{focus_none}").unwrap();
    let cs_running = crx.recv_timeout(Duration::from_secs(10)).unwrap();
    let cs_focus = crx.recv_timeout(Duration::from_secs(10)).unwrap();
    let _ = child.kill();

    // nova-sys
    let (rtx, rrx) = mpsc::channel::<Value>();
    let bridge = nova_sys::start(move |l| {
        if let Ok(v) = serde_json::from_str::<Value>(&l) {
            if v.get("running").is_some() || v.get("focusapp").is_some() {
                let _ = rtx.send(v);
            }
        }
    });
    bridge.send(cmd).unwrap();
    bridge.send(focus_none.into()).unwrap();
    let rs_running = rrx.recv_timeout(Duration::from_secs(10)).unwrap();
    let rs_focus = rrx.recv_timeout(Duration::from_secs(10)).unwrap();

    println!("{} apps asked about", pairs.len());
    println!("C#   running: {}", cs_running["running"]);
    println!("Rust running: {}", rs_running["running"]);
    println!("running lists {}", if cs_running == rs_running { "IDENTICAL" } else { "DIFFER" });
    println!("focusapp (no window): C# {} | Rust {} -> {}", cs_focus, rs_focus, if cs_focus == rs_focus { "IDENTICAL" } else { "DIFFER" });
}
