//! Prototype: Nova's desktop widgets as widgets.json in a Nova data folder sets them (desktop.rs).
//!   cargo run --release -p nova-render --example widgets -- [data folder] [options]
//!     --fresh                     starts from an empty desktop (widgets.json's places kept)
//!     --group board               adds a ready group (desktop.rs GROUPS)
//!     --add moon,pulse            adds widgets on their own
//!     --on-top                    above the windows (to photograph them)
//!     --test-explorer-restart     closes a widget as Explorer's exit does, then "brings Explorer back"
use std::path::PathBuf;

use serde_json::json;
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, TranslateMessage, MSG};

fn main() {
    nova_render::widget::dpi_aware();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    if args.iter().any(|a| a == "--on-top") {
        nova_render::widget::TEST_ON_TOP.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let takes_value = ["--group", "--add"];
    let dir = args
        .iter()
        .enumerate()
        .find(|(i, a)| !a.starts_with("--") && (*i == 0 || !takes_value.contains(&args[i - 1].as_str())))
        .map(|(_, a)| PathBuf::from(a))
        .unwrap_or_else(|| PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("dynamic-nova"));

    if args.iter().any(|a| a == "--fresh") {
        let mut v = nova_render::desktop::read(&dir);
        v["groups"] = json!([]);
        v["free"] = json!([]);
        nova_render::desktop::write(&dir, &v);
    }
    if let Some(g) = opt("--group") {
        nova_render::desktop::apply(&dir, &json!({ "addGroup": g }));
    }
    for id in opt("--add").unwrap_or_default().split(',').filter(|x| !x.is_empty()) {
        nova_render::desktop::apply(&dir, &json!({ "add": id }));
    }
    nova_render::desktop::show(&dir);

    if args.iter().any(|a| a == "--test-explorer-restart") {
        use windows::{core::w, Win32::{Foundation::{LPARAM, WPARAM}, UI::WindowsAndMessaging::*}};
        unsafe {
            let h = FindWindowW(w!("NovaWidget"), None).expect("widget window");
            let _ = DestroyWindow(h);
            let watch = FindWindowW(w!("NovaWidgetWatch"), None).expect("watcher");
            let _ = SendMessageW(watch, RegisterWindowMessageW(w!("TaskbarCreated")), Some(WPARAM(0)), Some(LPARAM(0)));
            let back = FindWindowW(w!("NovaWidget"), None);
            println!("after TaskbarCreated: widget window there = {}, visible = {}", back.is_ok(), back.map(|h| IsWindowVisible(h).as_bool()).unwrap_or(false));
        }
        return;
    }
    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
