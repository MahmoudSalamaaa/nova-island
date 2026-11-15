//! Other apps' notifications on the island (new in 1.8; "appNotify", off by default). The
//! notification center is read over and over (about 5 s a read, outside the Store); whatever is new
//! since the last read goes to the island as "app-notify". What was there when it was turned on is
//! not shown. Nothing is kept or logged.
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use serde_json::json;

use crate::ctx::{Ctx, Wake};

pub struct AppNotify {
    ctx: Ctx,
    /// the app of each notification shown lately (id -> AppUserModelId), for "Open"
    apps: Mutex<HashMap<u32, String>>,
    enabled: Mutex<bool>,
    wake: Wake,
}

impl AppNotify {
    pub fn new(ctx: Ctx) -> Arc<AppNotify> {
        let on = ctx.on("appNotify");
        Arc::new(AppNotify { ctx, apps: Mutex::new(HashMap::new()), enabled: Mutex::new(on), wake: Wake::new() })
    }

    pub fn start(self: &Arc<Self>) {
        let me = self.clone();
        std::thread::Builder::new()
            .name("nova-appnotify".into())
            .spawn(move || {
                unsafe {
                    let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
                }
                let mut seen: Option<HashSet<u32>> = None;
                // each app's icon and colour, read once
                let mut icons: HashMap<String, Option<(String, [u8; 3])>> = HashMap::new();
                loop {
                    if !*me.enabled.lock().unwrap() {
                        seen = None;
                        me.wake.sleep(60 * 60 * 1000);
                        continue;
                    }
                    // one read at a time: Windows refuses reads that overlap
                    if let Some(list) = nova_sys::notify::read() {
                        if let Some(old) = &seen {
                            let new: Vec<_> = list.iter().filter(|n| !old.contains(&n.id) && n.app != "Nova Island").collect();
                            // the newest one, and how many more came with it
                            if let Some(n) = new.iter().max_by_key(|n| n.id) {
                                let icon = icons.entry(n.app.clone()).or_insert_with(|| nova_sys::notify::icon(n)).clone();
                                let (icon, rgb) = icon.map(|(i, c)| (json!(i), json!(c))).unwrap_or((json!(""), json!(null)));
                                {
                                    let mut apps = me.apps.lock().unwrap();
                                    if apps.len() > 50 {
                                        apps.clear();
                                    }
                                    apps.insert(n.id, nova_sys::notify::aumid(n));
                                }
                                me.ctx.send("app-notify", json!({ "id": n.id, "app": n.app, "title": n.title, "body": n.body, "more": new.len() - 1, "icon": icon, "rgb": rgb }));
                            }
                        }
                        seen = Some(list.iter().map(|n| n.id).collect());
                    }
                    me.wake.sleep(1000);
                }
            })
            .expect("nova-appnotify thread");
    }

    /// Opens the app a shown notification came from. The page names the notification, never the
    /// app: only what was read from Windows is opened.
    pub fn open(&self, id: u32) {
        let Some(aumid) = self.apps.lock().unwrap().get(&id).cloned() else { return };
        if aumid.is_empty() || aumid.contains(['"', '\n', '\r']) {
            return;
        }
        let target = format!("shell:AppsFolder\\{aumid}");
        unsafe {
            windows::Win32::UI::Shell::ShellExecuteW(None, &windows::core::HSTRING::from("open"), &windows::core::HSTRING::from("explorer.exe"), &windows::core::HSTRING::from(target), None, windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL);
        }
    }

    pub fn set_enabled(&self, on: bool) {
        *self.enabled.lock().unwrap() = on;
        self.wake.wake();
    }
}
