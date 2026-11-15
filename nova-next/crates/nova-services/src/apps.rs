//! App launcher (apps.js): the Start menu catalog, pinned apps, icons, launching. A click brings the
//! app's open window to the front and only starts the app when none is open; a right-click (fresh)
//! always starts a new one.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use serde_json::{json, Value};

use crate::ctx::Ctx;

const DEFAULTS: &[&str] = &["WhatsApp", "Telegram", "Spotify", "Google Chrome", "Visual Studio Code", "Discord", "Microsoft Edge", "File Explorer", "Calculator", "Notepad", "Photos", "Settings"];
pub const MAX_PINNED: usize = 36; // 1.8: three pages of 6 x 2 (1.7: 12)

/// String#localeCompare: ICU collation (the en locale), as Chromium sorts.
pub fn locale_compare(a: &str, b: &str) -> std::cmp::Ordering {
    use windows::{core::PCSTR, Win32::Globalization::*};
    thread_local! {
        static COLL: *mut UCollator = unsafe {
            let mut st = U_ZERO_ERROR;
            ucol_open(PCSTR(c"en".as_ptr() as *const u8), &mut st)
        };
    }
    let (a16, b16): (Vec<u16>, Vec<u16>) = (a.encode_utf16().collect(), b.encode_utf16().collect());
    COLL.with(|c| {
        if c.is_null() {
            return a.cmp(b);
        }
        match unsafe { ucol_strcoll(*c, a16.as_ptr(), a16.len() as i32, b16.as_ptr(), b16.len() as i32) } {
            UCOL_LESS => std::cmp::Ordering::Less,
            UCOL_GREATER => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        }
    })
}

pub struct Apps {
    ctx: Ctx,
    catalog: Mutex<Vec<Value>>,
    pinned: Mutex<Option<Vec<String>>>,
    icons: Mutex<HashMap<String, String>>,
    running_busy: Mutex<bool>,
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}

impl Apps {
    pub fn new(ctx: Ctx) -> Arc<Apps> {
        let store = ctx.store();
        let catalog = store.load("apps-cache.json").and_then(|v| v.as_array().cloned()).unwrap_or_default();
        let pinned = store.load("apps.json").and_then(|v| v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()));
        Arc::new(Apps { ctx, catalog: Mutex::new(catalog), pinned: Mutex::new(pinned), icons: Mutex::new(HashMap::new()), running_busy: Mutex::new(false) })
    }

    /// The list sent as "apps" whenever the pins change.
    fn changed(&self) {
        let list = self.list();
        self.ctx.send("apps", list);
    }

    /// Scans the Start menu (3 tries, 3 s apart), then settles the pins.
    pub fn init(self: &Arc<Self>) {
        let me = self.clone();
        std::thread::spawn(move || {
            if !me.catalog.lock().unwrap().is_empty() && me.pinned.lock().unwrap().is_some() {
                me.changed();
            }
            for i in 0..3 {
                if me.scan_once() {
                    break;
                }
                if i < 2 {
                    std::thread::sleep(Duration::from_secs(3));
                }
            }
            // (1.8.7) catalog, then pinned: the order add() takes them in (the other way round, an
            // app added during the first scan left both threads waiting on each other)
            let catalog = me.catalog.lock().unwrap();
            let mut pinned = me.pinned.lock().unwrap();
            let mut list = pinned.take().unwrap_or_else(|| Self::pick_defaults(&catalog));
            list.retain(|id| catalog.iter().any(|a| s(a, "id") == id));
            *pinned = Some(list);
            drop((pinned, catalog));
            me.persist();
        });
    }

    fn scan_once(&self) -> bool {
        let Ok(list) = serde_json::from_str::<Value>(&nova_sys::apps::scan()) else {
            nova_core::log::error("[apps] scan failed: not JSON");
            return false;
        };
        let mut items: Vec<Value> = match list {
            Value::Array(a) => a,
            other => vec![other],
        };
        items.retain(|a| !s(a, "name").is_empty() && !s(a, "id").is_empty());
        if items.is_empty() {
            nova_core::log::error("[apps] scan failed: empty list");
            return false;
        }
        items.sort_by(|a, b| locale_compare(s(a, "name"), s(b, "name")));
        let _ = self.ctx.store().save("apps-cache.json", &Value::Array(items.clone()));
        *self.catalog.lock().unwrap() = items;
        true
    }

    fn pick_defaults(catalog: &[Value]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for want in DEFAULTS {
            let w = want.to_lowercase();
            let hit = catalog.iter().find(|a| s(a, "name").to_lowercase() == w).or_else(|| catalog.iter().find(|a| s(a, "name").to_lowercase().starts_with(&w)));
            if let Some(h) = hit {
                let id = s(h, "id").to_string();
                if !out.contains(&id) {
                    out.push(id);
                }
            }
            if out.len() >= MAX_PINNED {
                break;
            }
        }
        out
    }

    fn icon_for(&self, entry: &Value) -> String {
        let id = s(entry, "id").to_string();
        if s(entry, "icon").is_empty() {
            return String::new();
        }
        if let Some(i) = self.icons.lock().unwrap().get(&id) {
            return i.clone();
        }
        let url = if s(entry, "kind") == "png" { nova_sys::icons::png_scaled(s(entry, "icon"), 64) } else { nova_sys::icons::file_icon(s(entry, "icon")) };
        self.icons.lock().unwrap().insert(id, url.clone());
        url
    }

    fn describe(&self, id: &str) -> Option<Value> {
        let entry = self.catalog.lock().unwrap().iter().find(|a| s(a, "id") == id).cloned()?;
        Some(json!({ "id": id, "name": s(&entry, "name"), "icon": self.icon_for(&entry) }))
    }

    pub fn list(&self) -> Value {
        let pinned = self.pinned.lock().unwrap().clone().unwrap_or_default();
        Value::Array(pinned.iter().filter_map(|id| self.describe(id)).collect())
    }

    pub fn all(&self) -> Value {
        let catalog = self.catalog.lock().unwrap().clone();
        Value::Array(catalog.iter().map(|a| json!({ "id": s(a, "id"), "name": s(a, "name"), "icon": self.icon_for(a) })).collect())
    }

    fn persist(&self) {
        let p = self.pinned.lock().unwrap().clone();
        let _ = self.ctx.store().save("apps.json", &p.map(|v| json!(v)).unwrap_or(Value::Null));
        self.changed();
    }

    pub fn add(&self, id: &str) {
        {
            let catalog = self.catalog.lock().unwrap();
            let mut pinned = self.pinned.lock().unwrap();
            let list = pinned.get_or_insert_with(Vec::new);
            if !catalog.iter().any(|a| s(a, "id") == id) || list.iter().any(|p| p == id) || list.len() >= MAX_PINNED {
                return;
            }
            list.push(id.to_string());
        }
        self.persist();
    }

    pub fn remove(&self, id: &str) {
        {
            let mut pinned = self.pinned.lock().unwrap();
            let list = pinned.get_or_insert_with(Vec::new);
            list.retain(|p| p != id);
        }
        self.persist();
    }

    /// New order from the renderer; must be the same set of ids.
    pub fn reorder(&self, ids: &Value) {
        let Some(ids) = ids.as_array() else { return };
        let ids: Vec<String> = ids.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
        {
            let mut pinned = self.pinned.lock().unwrap();
            let Some(list) = pinned.as_mut() else { return };
            // (1.8.7) the same ids, each once: [A, A] for [A, B] dropped B
            let unique = ids.iter().enumerate().all(|(n, i)| !ids[..n].contains(i));
            if ids.len() != list.len() || !unique || !ids.iter().all(|i| list.contains(i)) {
                return;
            }
            *list = ids;
        }
        self.persist();
    }

    fn clean(v: &str) -> String {
        v.chars().filter(|c| !matches!(c, '\t' | '\r' | '\n')).collect()
    }

    fn exe_of(a: &Value) -> String {
        if s(a, "kind") == "file" && s(a, "icon").to_lowercase().ends_with(".exe") { s(a, "icon").to_string() } else { String::new() }
    }

    pub fn launch(&self, id: &str, fresh: bool) {
        let Some(a) = self.catalog.lock().unwrap().iter().find(|x| s(x, "id") == id).cloned() else { return };
        if !fresh && nova_sys::appwin::focus(&Self::clean(id), &Self::clean(&Self::exe_of(&a))) == "focused" {
            return;
        }
        let _ = std::process::Command::new("explorer.exe").arg(format!("shell:AppsFolder\\{id}")).spawn();
    }

    /// Pinned apps with a window open (the dot under their icon), asked when the Apps tab opens.
    pub fn running(&self) -> Value {
        let mut busy = self.running_busy.lock().unwrap();
        if *busy {
            return json!([]);
        }
        *busy = true;
        drop(busy);
        let pinned = self.pinned.lock().unwrap().clone().unwrap_or_default();
        let catalog = self.catalog.lock().unwrap().clone();
        let pairs: Vec<(String, String)> = pinned.iter().filter_map(|id| catalog.iter().find(|a| s(a, "id") == id)).map(|a| (Self::clean(s(a, "id")), Self::clean(&Self::exe_of(a)))).collect();
        let on = nova_sys::appwin::running(&pairs);
        *self.running_busy.lock().unwrap() = false;
        json!(on)
    }
}
