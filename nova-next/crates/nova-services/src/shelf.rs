//! File shelf (shelf.js): files dropped on the island, kept as paths (nothing is copied).
//! Screenshots added on their own carry an expiry time: they leave the shelf then (the file itself
//! is never touched).
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use serde_json::{json, Value};

use crate::ctx::{now_ms, Ctx};

const MAX_ITEMS: usize = 24;
const EXPIRE_CHECK_MS: u64 = 30 * 1000;

#[derive(Clone)]
struct Item {
    path: String,
    until: f64, // 0: it stays
}

pub struct Shelf {
    ctx: Ctx,
    items: Mutex<Vec<Item>>,
    icons: Mutex<HashMap<String, String>>,
}

fn exists(p: &str) -> bool {
    Path::new(p).exists()
}

impl Shelf {
    pub fn new(ctx: Ctx) -> Arc<Shelf> {
        // { path, until? } (older versions saved plain paths)
        let saved = ctx.store().load("shelf.json").and_then(|v| v.as_array().cloned()).unwrap_or_default();
        let items = saved
            .iter()
            .filter_map(|x| match x {
                Value::String(p) => Some(Item { path: p.clone(), until: 0.0 }),
                Value::Object(_) => x["path"].as_str().map(|p| Item { path: p.to_string(), until: x["until"].as_f64().unwrap_or(0.0) }),
                _ => None,
            })
            .filter(|i| exists(&i.path))
            .collect();
        Arc::new(Shelf { ctx, items: Mutex::new(items), icons: Mutex::new(HashMap::new()) })
    }

    pub fn start(self: &Arc<Self>) {
        let me = self.clone();
        std::thread::spawn(move || loop {
            me.expire();
            std::thread::sleep(Duration::from_millis(EXPIRE_CHECK_MS));
        });
    }

    fn changed(&self) {
        let l = self.list();
        self.ctx.send("shelf", l);
    }

    pub fn has(&self, p: &str) -> bool {
        self.items.lock().unwrap().iter().any(|x| x.path == p)
    }

    pub fn list(&self) -> Value {
        let items: Vec<Item> = {
            let mut it = self.items.lock().unwrap();
            it.retain(|x| exists(&x.path));
            it.clone()
        };
        Value::Array(
            items
                .iter()
                .map(|i| {
                    let icon = self.icons.lock().unwrap().entry(i.path.clone()).or_insert_with(|| nova_sys::icons::file_thumb(&i.path, 88)).clone();
                    let name = Path::new(&i.path).file_name().map(|n| n.to_string_lossy().into_owned()).filter(|n| !n.is_empty()).unwrap_or_else(|| i.path.clone());
                    json!({ "path": i.path, "name": name, "icon": icon, "until": i.until })
                })
                .collect(),
        )
    }

    fn save(&self) {
        let v: Vec<Value> = self.items.lock().unwrap().iter().map(|i| if i.until > 0.0 { json!({ "path": i.path, "until": i.until }) } else { json!({ "path": i.path }) }).collect();
        let _ = self.ctx.store().save("shelf.json", &Value::Array(v));
    }

    /// until: when the file leaves the shelf on its own (ms); 0 = it stays
    pub fn add(&self, paths: &[String], until: f64) {
        {
            let mut items = self.items.lock().unwrap();
            for p in paths {
                if p.is_empty() || !exists(p) {
                    continue;
                }
                items.retain(|x| x.path != *p);
                items.insert(0, Item { path: p.clone(), until });
            }
            // over the limit: the oldest timed screenshots go first, then the oldest files
            while items.len() > MAX_ITEMS {
                let i = items.iter().rposition(|x| x.until > 0.0).unwrap_or(items.len() - 1);
                items.remove(i);
            }
        }
        self.save();
        self.changed();
    }

    pub fn remove(&self, p: &str) {
        self.items.lock().unwrap().retain(|x| x.path != p);
        self.save();
        self.changed();
    }

    pub fn clear(&self) {
        self.items.lock().unwrap().clear();
        self.save();
        self.changed();
    }

    /// Timed items whose time is up leave the shelf (also those that ran out while the app was closed).
    fn expire(&self) {
        let now = now_ms();
        let gone = {
            let mut items = self.items.lock().unwrap();
            let before = items.len();
            items.retain(|x| x.until == 0.0 || x.until > now);
            before != items.len()
        };
        if gone {
            self.save();
            self.changed();
        }
    }
}
