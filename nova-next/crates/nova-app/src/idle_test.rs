// Tests only: what WebView2 costs while the island is closed, and how
// long it takes to come back. Nothing here runs unless the app starts with --test-idle:
//   --test-idle suspend   the page hidden and suspended (TrySuspend)
//   --test-idle low       the same, with WebView2's low memory target first
//   --test-idle close     the page closed for good (made again to wake)
//   --test-idle-at 45     seconds after start the mode is set (the page has settled)
//   --test-wake-at 150    seconds after start the page comes back; the log says how long it took
// The log lines start with "idle-test:".
use std::{cell::RefCell, rc::Rc, time::Instant};

use webview2_com::{Microsoft::Web::WebView2::Win32::*, TrySuspendCompletedHandler};
use windows::{core::Interface, Win32::{Foundation::RECT, UI::WindowsAndMessaging::GetClientRect}};

use crate::App;

fn log(s: &str) {
    nova_core::log::info(&format!("idle-test: {s}"));
}

/// Starts the test when the app was started with --test-idle (or --test-light-hover-at S: the
/// island is "hovered" at S seconds and left 3 s later, as if the cursor came and went, so the
/// island's hand-off to the page and back can be checked without moving the mouse).
pub fn start(a: &Rc<RefCell<App>>) {
    if let Some(at) = crate::arg("--test-light-hover-at").and_then(|v| v.parse::<u64>().ok()) {
        let bus = a.borrow().bus.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(at));
            bus.ui(|a| {
                log("hover on");
                (a.borrow().core.send)("hover", serde_json::json!(true));
            });
            // --test-light-hover-hold S: left open S seconds instead (to measure it open)
            let hold = crate::arg("--test-light-hover-hold").and_then(|v| v.parse::<u64>().ok()).unwrap_or(3);
            std::thread::sleep(std::time::Duration::from_secs(hold));
            bus.ui(|a| {
                log("hover off");
                (a.borrow().core.send)("hover", serde_json::json!(false));
            });
        });
    }
    // --test-open-tab ID: the open island's tab is ID (set in the page 4 s after the start)
    if let Some(tab) = crate::arg("--test-open-tab") {
        let bus = a.borrow().bus.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(4));
            bus.ui(move |a| {
                log(&format!("open tab {tab}"));
                if let Some(p) = a.borrow().page.clone() {
                    p.run(&format!("typeof selectTab === 'function' && selectTab('{tab}')"));
                }
            });
        });
    }
    // --test-light-dim-at S: the island fades at S seconds (as over a browser's tabs) for 2 s
    if let Some(at) = crate::arg("--test-light-dim-at").and_then(|v| v.parse::<u64>().ok()) {
        let bus = a.borrow().bus.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(at));
            bus.ui(|a| (a.borrow().core.send)(crate::core::DIM, serde_json::json!(true)));
            std::thread::sleep(std::time::Duration::from_secs(2));
            bus.ui(|a| (a.borrow().core.send)(crate::core::DIM, serde_json::json!(false)));
        });
    }
    // --test-event-at "S CHANNEL JSON[;S CHANNEL JSON...]": a service's event at S seconds, the
    // way a real one comes (a meeting, downloads, the microphone in use...)
    if let Some(spec) = crate::arg("--test-event-at") {
        let bus = a.borrow().bus.clone();
        let start = std::time::Instant::now();
        let mut plan: Vec<(u64, String, String)> = spec
            .split(';')
            .filter_map(|e| {
                let mut p = e.trim().splitn(3, ' ');
                Some((p.next()?.parse().ok()?, p.next()?.to_string(), p.next()?.to_string()))
            })
            .collect();
        plan.sort_by_key(|p| p.0);
        std::thread::spawn(move || {
            for (at, ch, json) in plan {
                let wait = std::time::Duration::from_secs(at).saturating_sub(start.elapsed());
                std::thread::sleep(wait);
                // "NOW+m" in the JSON: now plus m minutes (ms since 1970), for meetings
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
                let mut json = json.clone();
                while let Some(i) = json.find("NOW+") {
                    let end = json[i + 4..].find(|c: char| !c.is_ascii_digit()).map(|e| i + 4 + e).unwrap_or(json.len());
                    let m: i64 = json[i + 4..end].parse().unwrap_or(0);
                    json.replace_range(i..end, &(now + m * 60_000).to_string());
                }
                match serde_json::from_str::<serde_json::Value>(&json) {
                    Ok(v) => {
                        log(&format!("event test: {ch}"));
                        bus.event(&ch, v);
                    }
                    Err(e) => log(&format!("event test: bad JSON for {ch}: {e}")),
                }
            }
        });
    }
    let Some(mode) = crate::arg("--test-idle") else { return };
    if !["suspend", "low", "close"].contains(&mode.as_str()) {
        log(&format!("unknown mode {mode}"));
        return;
    }
    let secs = |name: &str, d: u64| crate::arg(name).and_then(|v| v.parse::<u64>().ok()).unwrap_or(d);
    let (at, wake) = (secs("--test-idle-at", 45), secs("--test-wake-at", 150));
    log(&format!("mode {mode}, idle at {at} s, wake at {wake} s"));
    let bus = a.borrow().bus.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(at));
        let m = mode.clone();
        bus.ui(move |a| idle(a, &m));
        std::thread::sleep(std::time::Duration::from_secs(wake.saturating_sub(at)));
        bus.ui(move |a| wake_up(a, &mode));
    });
}

fn idle(a: &Rc<RefCell<App>>, mode: &str) {
    let Some(page) = a.borrow().page.clone() else { return };
    unsafe {
        let _ = page.controller.SetIsVisible(false);
        if mode == "low" {
            match page.webview.cast::<ICoreWebView2_19>() {
                Ok(w) => log(&format!("low memory target: {:?}", w.SetMemoryUsageTargetLevel(COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW))),
                Err(e) => log(&format!("no ICoreWebView2_19: {e}")),
            }
        }
        if mode == "close" {
            let _ = page.controller.Close();
            a.borrow_mut().page = None;
            crate::clear_page_slot();
            log("page closed");
            return;
        }
        let Ok(w3) = page.webview.cast::<ICoreWebView2_3>() else { return log("no ICoreWebView2_3") };
        let t = Instant::now();
        let handler = TrySuspendCompletedHandler::create(Box::new(move |code, ok| {
            log(&format!("suspended: {ok} ({code:?}) in {} ms", t.elapsed().as_millis()));
            Ok(())
        }));
        if let Err(e) = w3.TrySuspend(&handler) {
            log(&format!("TrySuspend failed: {e}"));
        }
    }
}

fn wake_up(a: &Rc<RefCell<App>>, mode: &str) {
    let t = Instant::now();
    if mode == "close" {
        let (env, hwnd) = {
            let s = a.borrow();
            (s.env.clone(), s.core.win.hwnd)
        };
        let Some(env) = env else { return };
        let mut r = RECT::default();
        unsafe {
            let _ = GetClientRect(hwnd, &mut r);
        }
        let first = Rc::new(std::cell::Cell::new(false));
        let page = nova_web::Page::create(&env, hwnd, r.right, r.bottom, Some(nova_web::Bridge::Nova), move |msg| {
            if !first.replace(true) {
                log(&format!("first message from the new page after {} ms", t.elapsed().as_millis()));
            }
            if let Some(a) = crate::app() {
                crate::router::handle(&a, msg, crate::router::Src::Island);
            }
        });
        match page {
            Ok(p) => {
                let p = Rc::new(p);
                p.pin_scale(unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) } as f64 / 96.0);
                let _ = p.navigate(&format!("{}index.html", nova_web::ORIGIN));
                log(&format!("page made again in {} ms", t.elapsed().as_millis()));
                a.borrow_mut().page = Some(p.clone());
                crate::set_page_slot(p);
            }
            Err(e) => log(&format!("page not made: {e}")),
        }
        return;
    }
    let Some(page) = a.borrow().page.clone() else { return };
    unsafe {
        if mode == "low" {
            if let Ok(w) = page.webview.cast::<ICoreWebView2_19>() {
                let _ = w.SetMemoryUsageTargetLevel(COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL);
            }
        }
        if let Ok(w3) = page.webview.cast::<ICoreWebView2_3>() {
            let _ = w3.Resume();
        }
        let _ = page.controller.SetIsVisible(true);
    }
    // the page answers a script: it is running again
    let r = page.eval("1+1");
    log(&format!("resumed and answering ({r:?}) after {} ms", t.elapsed().as_millis()));
}
