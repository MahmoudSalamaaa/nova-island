// The sped-up leak test (instead of 12 hours): nova-sys ticking every 50 ms (10x faster than the
// app), plus the heavy one-off jobs again and again: the apps list, running apps, the YouTube link
// search, and harmless commands. Memory (private bytes) and handles are logged every minute; they
// must stay flat.
//   cargo run --release -p nova-sys --example soak -- <minutes>
use std::time::{Duration, Instant};

use windows::Win32::System::{
    ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX},
    Threading::{GetCurrentProcess, GetProcessHandleCount},
};

fn usage() -> (f64, u32) {
    unsafe {
        let mut c = PROCESS_MEMORY_COUNTERS_EX { cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32, ..Default::default() };
        let _ = GetProcessMemoryInfo(GetCurrentProcess(), &mut c as *mut _ as *mut _, c.cb);
        let mut h = 0u32;
        let _ = GetProcessHandleCount(GetCurrentProcess(), &mut h);
        (c.PrivateUsage as f64 / 1048576.0, h)
    }
}

fn main() {
    let minutes: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(60);
    let lines = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let l2 = lines.clone();
    let tx = nova_sys::start_every(move |_| { l2.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }, 50);
    let cache = std::path::PathBuf::from(std::env::var("APPDATA").unwrap()).join("dynamic-nova/apps-cache.json");
    let apps: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(cache).unwrap_or("[]".into())).unwrap_or_default();
    let running = format!("runningapps\t{}", apps.iter().take(40).map(|a| format!("{}\t", a["id"].as_str().unwrap_or(""))).collect::<String>());
    let start = Instant::now();
    let mut next_sample = start;
    let mut jobs = 0u64;
    println!("minute  private MB  handles  lines  jobs");
    while start.elapsed() < Duration::from_secs(minutes * 60) {
        if Instant::now() >= next_sample {
            let (mb, h) = usage();
            println!("{:>6}  {mb:>9.1}  {h:>7}  {:>5}  {jobs}", start.elapsed().as_secs() / 60, lines.load(std::sync::atomic::Ordering::Relaxed));
            next_sample += Duration::from_secs(60);
        }
        let _ = tx.send(running.clone());
        let _ = tx.send("seekapp No.Such.App 1".into());
        if jobs % 20 == 0 {
            let _ = nova_sys::apps::scan();
        }
        if jobs % 10 == 0 {
            let _ = nova_sys::yturl::yt_url("chrome", "zz no such video title 9431");
        }
        jobs += 1;
        std::thread::sleep(Duration::from_millis(1500));
    }
    let (mb, h) = usage();
    println!("end     {mb:>9.1}  {h:>7}  {:>5}  {jobs}", lines.load(std::sync::atomic::Ordering::Relaxed));
}
