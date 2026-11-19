//! Plugins (plugins.js): extra features a user adds only when they want them. The list comes from
//! the Nova plugin catalog on GitHub, its install counts from the release downloads there (no
//! tracking of anyone). A plugin is one signed file (.nvp); a changed or foreign file is refused,
//! from the catalog or from a file on disk alike. Plugins of this first version hold content only:
//! the island itself does the work for each kind ("radio": live Qur'an stations it plays).
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::ctx::{now_ms, Ctx};

const CATALOG_URL: &str = "https://raw.githubusercontent.com/NOVA-TECH-KIT/nova-island-plugins/main/plugins.json";
const RELEASES_URL: &str = "https://api.github.com/repos/NOVA-TECH-KIT/nova-island-plugins/releases?per_page=100";
const CATALOG_MS: f64 = 6.0 * 60.0 * 60.0 * 1000.0;
const MAX_BYTES: usize = 2 * 1024 * 1024;

/// The catalog that ships with the app (plugins-catalog.json, signed like the online one).
const BUILTIN: &str = include_str!("../../../../plugins-catalog.json");
/// Quran Radio comes with 1.8, installed and on; the user can switch it off or remove it (then it
/// stays removed until they install it again). Signed like any plugin, and newer than the catalog's.
pub const RADIO_ID: &str = "quran-radio";
const RADIO_BUNDLE: &str = include_str!("../../../plugins/quran-radio.nvp");
/// Tasks (1.8.3): the to-do list, built in like the radio
pub const TASKS_ID: &str = "tasks";
const TASKS_BUNDLE: &str = include_str!("../../../plugins/tasks.nvp");
/// The plugins that come with the app: installed and on unless the user removed them.
const BUILTINS: &[(&str, &str)] = &[(RADIO_ID, RADIO_BUNDLE), (TASKS_ID, TASKS_BUNDLE)];

fn builtin_bundle(id: &str) -> Option<&'static str> {
    BUILTINS.iter().find(|(i, _)| *i == id).map(|(_, b)| *b)
}

fn id_ok(s: &str) -> bool {
    (2..=40).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn semver_ok(s: &str) -> bool {
    let p: Vec<&str> = s.split('.').collect();
    p.len() == 3 && p.iter().all(|x| !x.is_empty() && x.bytes().all(|b| b.is_ascii_digit()))
}

fn word_dot(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}

/// ^https://github.com/NOVA-TECH-KIT/nova-island-plugins/releases/download/<tag>/<file>.nvp$
fn asset_url_ok(u: &str) -> bool {
    let Some(rest) = u.strip_prefix("https://github.com/NOVA-TECH-KIT/nova-island-plugins/releases/download/") else { return false };
    let Some((tag, file)) = rest.split_once('/') else { return false };
    word_dot(tag) && file.ends_with(".nvp") && word_dot(file) && file.len() > 4
}

/// the hosts a radio plugin may stream from (the island's page allows exactly these, too)
fn radio_url_ok(u: &str) -> bool {
    let Some(rest) = u.strip_prefix("https://") else { return false };
    let Some((host, path)) = rest.split_once('/') else { return false };
    let label = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    let host_ok = host.strip_suffix(".radiojar.com").is_some_and(label) || host == "qurango.net" || host.strip_suffix(".qurango.net").is_some_and(label);
    host_ok && !path.is_empty() && path.bytes().all(|b| b.is_ascii_alphanumeric() || b"_./?=&-".contains(&b))
}

/// text(v, max): a trimmed string cut to `max` UTF-16 units, "" for anything else.
fn text(v: &Value, max: usize) -> String {
    v.as_str().map(|s| String::from_utf16_lossy(&s.trim_matches(crate::smartclip::js_space).encode_utf16().take(max).collect::<Vec<_>>())).unwrap_or_default()
}

/// A number as JavaScript keeps it: whole numbers stay integers in JSON.
fn js_num(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 9e15 { json!(n as i64) } else { json!(n) }
}

/// Number(v) || d
fn num_or(v: &Value, d: f64) -> f64 {
    let n = match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.trim().parse().unwrap_or(f64::NAN),
        Value::Bool(b) => *b as i32 as f64,
        Value::Null => 0.0,
        _ => f64::NAN,
    };
    if n == 0.0 || n.is_nan() { d } else { n }
}

fn check_radio(m: &Value) -> Option<Map<String, Value>> {
    let list = m["stations"].as_array()?;
    if list.is_empty() || list.len() > 500 {
        return None;
    }
    let mut stations = Vec::new();
    for s in list {
        let name = text(&s["name"], 80);
        if name.is_empty() || !id_ok(s["id"].as_str().unwrap_or("")) || !radio_url_ok(s["url"].as_str().unwrap_or("")) {
            return None;
        }
        let mut st = json!({ "id": s["id"], "name": name, "url": s["url"], "group": text(&s["group"], 40) });
        // 1.8: a reciter's surahs, each played from its own file on mp3quran's server
        if let Some(su) = s.get("surahs") {
            st["surahs"] = check_surahs(su)?;
        }
        stations.push(st);
    }
    let mut out = Map::new();
    out.insert("stations".into(), Value::Array(stations));
    Some(out)
}

/// {server: "https://cdn.mp3quran.net/audio/<name>/<r>/", rewaya, list: [1..114, ascending]}
fn check_surahs(su: &Value) -> Option<Value> {
    let server = su["server"].as_str()?;
    let path = server.strip_prefix("https://cdn.mp3quran.net/audio/")?;
    if !path.ends_with('/') || path.len() < 2 || path.contains("..") || !path.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-/".contains(&b)) {
        return None;
    }
    let list: Vec<i64> = su["list"].as_array()?.iter().map(|n| n.as_i64().filter(|n| (1..=114).contains(n))).collect::<Option<_>>()?;
    if list.is_empty() || list.windows(2).any(|w| w[0] >= w[1]) {
        return None;
    }
    Some(json!({ "server": server, "rewaya": text(&su["rewaya"], 60), "list": list }))
}

fn check_settings(list: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    for s in list.as_array().map(|a| a.iter().take(12).collect::<Vec<_>>()).unwrap_or_default() {
        let key = s["key"].as_str().unwrap_or("");
        let ty = s["type"].as_str().unwrap_or("");
        if !(1..=30).contains(&key.len()) || !key.bytes().all(|b| b.is_ascii_alphabetic()) || !matches!(ty, "bool" | "station" | "range") {
            continue;
        }
        let mut item = json!({ "key": key, "type": ty, "label": text(&s["label"], 60), "hint": text(&s["hint"], 120) });
        if let Some(d) = s.get("default") {
            item["default"] = d.clone();
        }
        if ty == "range" {
            item["min"] = js_num(num_or(&s["min"], 0.0));
            item["max"] = js_num(num_or(&s["max"], 100.0));
            item["step"] = js_num(num_or(&s["step"], 10.0));
        }
        out.push(item);
    }
    out
}

/// Validate Focus Timer configuration independently of bundle signature verification.
/// The timer has no executable payload or network permissions.
fn check_focus_timer(m: &Value) -> Option<Map<String, Value>> {
    let settings = m["settings"].as_array()?;
    if settings.len() != 2 { return None; }
    for (key, min, max, default) in [("focusMinutes", 5, 120, 25), ("breakMinutes", 1, 30, 5)] {
        let matches: Vec<&Value> = settings.iter().filter(|s| s["key"] == key).collect();
        if matches.len() != 1 { return None; }
        let setting = matches[0];
        if setting["type"] != "range" || setting["min"] != min || setting["max"] != max || setting["default"] != default { return None; }
        let step = setting["step"].as_i64()?;
        if step < 1 || step > 5 { return None; }
    }
    Some(Map::new())
}

/// A bundle is {manifest, sig}; returns the clean manifest, or None.
pub fn check_bundle(bundle: &Value) -> Option<Value> {
    let m = bundle.get("manifest")?;
    if !m.is_object() || !nova_sign::verify(m, bundle["sig"].as_str()) {
        return None;
    }
    if !id_ok(m["id"].as_str().unwrap_or("")) || !semver_ok(m["version"].as_str().unwrap_or("")) {
        return None;
    }
    // a radio carries its stations; Tasks (1.8.3) only turns the app's own to-do list on
    let content = match m["kind"].as_str() {
        Some("radio") => check_radio(m)?,
        Some("tasks") => Map::new(),
        Some("focus-timer") => check_focus_timer(m)?,
        _ => return None,
    };
    let mut out = json!({ "id": m["id"], "kind": m["kind"], "version": m["version"], "name": text(&m["name"], 40), "description": text(&m["description"], 160), "settings": check_settings(&m["settings"]) });
    for (k, v) in content {
        out[k] = v;
    }
    Some(out)
}

fn clean_catalog(list: &Value) -> Vec<Value> {
    list.as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|p| id_ok(p["id"].as_str().unwrap_or("")) && asset_url_ok(p["url"].as_str().unwrap_or("")) && semver_ok(p["version"].as_str().unwrap_or("")) && p["price"] == "free")
        .map(|p| {
            let sha = p["sha256"].as_str().filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))).unwrap_or("");
            json!({ "id": p["id"], "name": text(&p["name"], 40), "description": text(&p["description"], 160), "version": p["version"], "size": js_num(num_or(&p["size"], 0.0)), "price": "free", "url": p["url"], "sha256": sha })
        })
        .collect()
}

fn newer(a: &str, b: &str) -> bool {
    let (pa, pb): (Vec<i64>, Vec<i64>) = (a.split('.').map(|x| x.parse().unwrap_or(0)).collect(), b.split('.').map(|x| x.parse().unwrap_or(0)).collect());
    for i in 0..3 {
        if pa.get(i) != pb.get(i) {
            return pa.get(i).unwrap_or(&0) > pb.get(i).unwrap_or(&0);
        }
    }
    false
}

struct State {
    state: Map<String, Value>, // id -> {enabled, settings}
    catalog: Vec<Value>,
    counts: Map<String, Value>,
    at: f64,
    installed: HashMap<String, Value>, // id -> manifest, in load order
    order: Vec<String>,
    catalog_error: String,
    busy: HashSet<String>,
}

pub struct Plugins {
    ctx: Ctx,
    st: Mutex<State>,
}

impl Plugins {
    pub fn new(ctx: Ctx) -> Arc<Plugins> {
        let store = ctx.store();
        let state = store.load("plugins.json").and_then(|v| v.as_object().cloned()).unwrap_or_default();
        let cache = store.load("plugins-cache.json").unwrap_or(json!({ "catalog": [], "counts": {}, "at": 0 }));
        let mut catalog = cache["catalog"].as_array().cloned().unwrap_or_default();
        if catalog.is_empty() {
            let cat: Value = serde_json::from_str(BUILTIN).unwrap_or_default();
            if nova_sign::verify(&json!({ "plugins": cat["plugins"] }), cat["sig"].as_str()) {
                catalog = clean_catalog(&cat["plugins"]);
            }
        }
        let st = State { state, catalog, counts: cache["counts"].as_object().cloned().unwrap_or_default(), at: cache["at"].as_f64().unwrap_or(0.0), installed: HashMap::new(), order: Vec::new(), catalog_error: String::new(), busy: HashSet::new() };
        let me = Arc::new(Plugins { ctx, st: Mutex::new(st) });
        me.load_installed();
        me.builtins();
        me
    }

    fn dir(&self) -> PathBuf {
        self.ctx.dir.join("plugins")
    }

    fn load_installed(&self) {
        let mut names: Vec<String> = std::fs::read_dir(self.dir()).map(|r| r.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".nvp")).collect()).unwrap_or_default();
        names.sort();
        let mut st = self.st.lock().unwrap();
        st.installed.clear();
        st.order.clear();
        for n in names {
            let Ok(text) = std::fs::read_to_string(self.dir().join(&n)) else { continue };
            let Ok(bundle) = serde_json::from_str::<Value>(&text) else { continue };
            match check_bundle(&bundle) {
                Some(m) if format!("{}.nvp", m["id"].as_str().unwrap_or("")) == n => {
                    let id = m["id"].as_str().unwrap().to_string();
                    st.order.push(id.clone());
                    st.installed.insert(id, m);
                }
                _ => nova_core::log::info(&format!("[plugins] {n} failed its check, not loaded")),
            }
        }
    }

    /// Quran Radio, installed and on unless the user removed it; an older copy (1.7's 1.0.0) is
    /// replaced, keeping its settings and whether it is on.
    fn builtins(&self) {
        for (id, text) in BUILTINS {
            self.builtin(id, text);
        }
    }

    fn builtin(&self, id: &str, text: &str) {
        let Ok(bundle) = serde_json::from_str::<Value>(text) else { return };
        let Some(m) = check_bundle(&bundle) else { return };
        let (removed, have) = {
            let st = self.st.lock().unwrap();
            (st.state.get(id).is_some_and(|s| s["removed"] == true), st.installed.get(id).map(|i| i["version"].as_str().unwrap_or("").to_string()))
        };
        match have {
            None if !removed => {
                let _ = self.save(&bundle, &m);
            }
            Some(v) if newer(m["version"].as_str().unwrap_or(""), &v) => {
                if std::fs::write(self.dir().join(format!("{id}.nvp")), bundle.to_string()).is_ok() {
                    self.st.lock().unwrap().installed.insert(id.into(), m.clone());
                    nova_core::log::info(&format!("[plugins] {id} {v} -> {}", m["version"].as_str().unwrap_or("")));
                }
            }
            _ => {}
        }
    }

    pub fn start(self: &Arc<Self>) {
        let a = self.active();
        self.ctx.send("plugins", a);
        let me = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(30));
            me.refresh(false);
        });
    }

    fn enabled(st: &State, id: &str) -> bool {
        st.state.get(id).is_none_or(|s| s["enabled"] != false)
    }

    fn values(st: &State, m: &Value) -> Value {
        let saved = st.state.get(m["id"].as_str().unwrap_or("")).map(|s| s["settings"].clone()).unwrap_or(json!({}));
        let mut out = Map::new();
        for s in m["settings"].as_array().cloned().unwrap_or_default() {
            let k = s["key"].as_str().unwrap_or("").to_string();
            let v = saved.get(&k).cloned().or_else(|| s.get("default").cloned());
            if let Some(v) = v {
                out.insert(k, v);
            }
        }
        Value::Object(out)
    }

    /// For the island: installed and switched on.
    pub fn active(&self) -> Value {
        let st = self.st.lock().unwrap();
        Value::Array(
            st.order.iter().filter_map(|id| st.installed.get(id)).filter(|m| Self::enabled(&st, m["id"].as_str().unwrap_or("")))
                .map(|m| json!({ "id": m["id"], "kind": m["kind"], "name": m["name"], "stations": m["stations"], "values": Self::values(&st, m) }))
                .collect(),
        )
    }

    /// For Settings > Plugins: the catalog and whatever is installed (also from a file).
    pub fn list(&self) -> Value {
        let st = self.st.lock().unwrap();
        let mut rows: Vec<(String, Value)> = st.catalog.iter().map(|c| {
            let mut r = c.clone();
            r["installs"] = st.counts.get(c["id"].as_str().unwrap_or("")).cloned().unwrap_or(Value::Null);
            (c["id"].as_str().unwrap_or("").to_string(), r)
        }).collect();
        for id in &st.order {
            let m = &st.installed[id];
            let c = rows.iter().position(|(k, _)| k == id);
            let mut r = match c {
                Some(i) => rows[i].1.clone(),
                None => json!({ "id": id, "name": m["name"], "description": m["description"], "version": m["version"], "size": 0, "installs": null }),
            };
            r["installed"] = m["version"].clone();
            r["update"] = json!(c.is_some() && newer(r["version"].as_str().unwrap_or(""), m["version"].as_str().unwrap_or("")));
            r["enabled"] = json!(Self::enabled(&st, id));
            r["settings"] = m["settings"].clone();
            r["values"] = Self::values(&st, m);
            r["stations"] = Value::Array(m["stations"].as_array().cloned().unwrap_or_default().iter().map(|s| json!({ "id": s["id"], "name": s["name"], "group": s["group"] })).collect());
            match c {
                Some(i) => rows[i].1 = r,
                None => rows.push((id.clone(), r)),
            }
        }
        let plugins: Vec<Value> = rows.into_iter().map(|(id, mut r)| { r["busy"] = json!(st.busy.contains(&id)); r }).collect();
        json!({ "plugins": plugins, "error": st.catalog_error })
    }

    fn changed(&self) {
        self.ctx.send("plugins", self.active());
        self.ctx.send("plugins-list", self.list());
    }

    /// The catalog from GitHub (at most every 6 hours unless forced) and the install counts.
    pub fn refresh(&self, force: bool) {
        if !force && now_ms() - self.st.lock().unwrap().at < CATALOG_MS {
            return;
        }
        let get = |url: &str| -> Result<Value, String> {
            let r = nova_core::http::request("GET", url, "accept: application/json\r\nuser-agent: Nova Island\r\n", None, 15000)?;
            if !r.ok() {
                return Err(format!("HTTP {}", r.status));
            }
            serde_json::from_slice(&r.body).map_err(|e| e.to_string())
        };
        let result = get(CATALOG_URL).and_then(|cat| {
            if !nova_sign::verify(&json!({ "plugins": cat["plugins"] }), cat["sig"].as_str()) {
                return Err("catalog signature".to_string());
            }
            let catalog = clean_catalog(&cat["plugins"]);
            let mut counts = Map::new();
            if let Ok(rels) = get(RELEASES_URL) {
                for rel in rels.as_array().cloned().unwrap_or_default() {
                    for a in rel["assets"].as_array().cloned().unwrap_or_default() {
                        let name = a["name"].as_str().unwrap_or("");
                        if let Some(id) = catalog.iter().map(|p| p["id"].as_str().unwrap_or("")).find(|id| name.starts_with(&format!("{id}-")) && name.ends_with(".nvp")) {
                            let n = counts.get(id).and_then(Value::as_f64).unwrap_or(0.0) + a["download_count"].as_f64().unwrap_or(0.0);
                            counts.insert(id.to_string(), js_num(n));
                        }
                    }
                }
            }
            Ok((catalog, counts))
        });
        {
            let mut st = self.st.lock().unwrap();
            match result {
                Ok((catalog, counts)) => {
                    st.catalog = catalog;
                    st.counts = counts;
                    st.at = now_ms();
                    let _ = self.ctx.store().save("plugins-cache.json", &json!({ "catalog": st.catalog, "counts": st.counts, "at": st.at }));
                    st.catalog_error.clear();
                }
                Err(msg) => {
                    if st.catalog_error != msg {
                        nova_core::log::info(&format!("[plugins] can't read the catalog: {msg}"));
                    }
                    st.catalog_error = if msg.contains("HTTP") || msg.contains("signature") { msg } else { "offline".into() };
                }
            }
        }
        self.changed();
    }

    fn save(&self, bundle: &Value, m: &Value) -> Result<(), String> {
        std::fs::create_dir_all(self.dir()).map_err(|e| e.to_string())?;
        let id = m["id"].as_str().unwrap_or("").to_string();
        std::fs::write(self.dir().join(format!("{id}.nvp")), bundle.to_string()).map_err(|e| e.to_string())?;
        let mut st = self.st.lock().unwrap();
        let settings = st.state.get(&id).map(|s| s["settings"].clone()).filter(|s| s.is_object()).unwrap_or(json!({}));
        st.state.insert(id.clone(), json!({ "enabled": true, "settings": settings }));
        let _ = self.ctx.store().save("plugins.json", &Value::Object(st.state.clone()));
        if !st.order.contains(&id) {
            st.order.push(id.clone());
        }
        st.installed.insert(id.clone(), m.clone());
        nova_core::log::info(&format!("[plugins] {id} {} installed", m["version"].as_str().unwrap_or("")));
        Ok(())
    }

    fn progress(&self, id: &str, mut data: Value) {
        data["id"] = json!(id);
        self.ctx.send("plugin-progress", data);
    }

    pub fn install(&self, id: &str) -> bool {
        // back after the user removed it: the copy that comes with the app (newer than the catalog's)
        if let Some(text) = builtin_bundle(id) {
            if let Some(m) = serde_json::from_str::<Value>(text).ok().and_then(|b| check_bundle(&b).map(|m| (b, m))).and_then(|(b, m)| self.save(&b, &m).ok().map(|_| m)) {
                self.progress(id, json!({ "pct": 1, "done": true, "name": m["name"] }));
                self.changed();
                return true;
            }
        }
        let entry = {
            let mut st = self.st.lock().unwrap();
            let Some(e) = st.catalog.iter().find(|p| p["id"].as_str() == Some(id)).cloned() else { return false };
            if !st.busy.insert(id.to_string()) {
                return false;
            }
            e
        };
        let name = entry["name"].clone();
        self.progress(id, json!({ "pct": 0, "name": name }));
        self.changed();
        let size = entry["size"].as_f64().unwrap_or(0.0) as usize;
        let result = (|| -> Result<Value, String> {
            let mut on = |got: usize, total: usize| {
                if got > MAX_BYTES {
                    return false;
                }
                let total = if total > 0 { total } else { size };
                if total > 0 {
                    self.progress(id, json!({ "pct": (got as f64 / total as f64).min(0.99), "name": name }));
                }
                true
            };
            let r = nova_core::http::request_progress("GET", entry["url"].as_str().unwrap_or(""), "user-agent: Nova Island\r\n", None, 60000, &mut on)?;
            if !r.ok() {
                return Err(format!("HTTP {}", r.status));
            }
            let sha = entry["sha256"].as_str().unwrap_or("");
            if !sha.is_empty() && format!("{:x}", Sha256::digest(&r.body)) != sha {
                return Err("checksum".into());
            }
            let bundle: Value = serde_json::from_slice(&r.body).map_err(|e| e.to_string())?;
            let m = check_bundle(&bundle).filter(|m| m["id"].as_str() == Some(id)).ok_or("signature")?;
            self.save(&bundle, &m)?;
            Ok(m)
        })();
        self.st.lock().unwrap().busy.remove(id);
        let ok = match result {
            Ok(m) => {
                self.progress(id, json!({ "pct": 1, "done": true, "name": m["name"] }));
                true
            }
            Err(e) => {
                nova_core::log::info(&format!("[plugins] {id} install failed: {e}"));
                self.progress(id, json!({ "error": true, "name": name }));
                false
            }
        };
        self.changed();
        ok
    }

    /// A .nvp file the user picked (offline, or a plugin not in the catalog yet).
    pub fn install_file(&self, file: &str) -> Value {
        if std::fs::metadata(file).map(|m| m.len() as usize > MAX_BYTES).unwrap_or(false) {
            return json!({ "ok": false, "error": "This file is too big for a plugin" });
        }
        let Some(bundle) = std::fs::read_to_string(file).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else {
            return json!({ "ok": false, "error": "This isn't a Nova Island plugin file" });
        };
        let Some(m) = check_bundle(&bundle) else {
            return json!({ "ok": false, "error": "This plugin isn't signed by Nova Island, so it can't be installed" });
        };
        if self.save(&bundle, &m).is_err() {
            return json!({ "ok": false, "error": "This isn't a Nova Island plugin file" });
        }
        self.progress(m["id"].as_str().unwrap_or(""), json!({ "pct": 1, "done": true, "name": m["name"] }));
        self.changed();
        json!({ "ok": true })
    }

    pub fn remove(&self, id: &str) {
        {
            let mut st = self.st.lock().unwrap();
            if !st.installed.contains_key(id) {
                return;
            }
            let _ = std::fs::remove_file(self.dir().join(format!("{id}.nvp")));
            st.installed.remove(id);
            st.order.retain(|x| x != id);
            st.state.remove(id);
            // a built-in plugin stays away until the user installs it again
            if builtin_bundle(id).is_some() {
                st.state.insert(id.into(), json!({ "removed": true }));
            }
            let _ = self.ctx.store().save("plugins.json", &Value::Object(st.state.clone()));
        }
        nova_core::log::info(&format!("[plugins] {id} removed"));
        self.changed();
    }

    pub fn set_enabled(&self, id: &str, on: bool) {
        {
            let mut st = self.st.lock().unwrap();
            if !st.installed.contains_key(id) {
                return;
            }
            let mut s = st.state.get(id).cloned().unwrap_or(json!({}));
            s["enabled"] = json!(on);
            st.state.insert(id.to_string(), s);
            let _ = self.ctx.store().save("plugins.json", &Value::Object(st.state.clone()));
        }
        self.changed();
    }

    /// A setting from Settings or the island, checked against the plugin's own list.
    pub fn set_setting(&self, id: &str, key: &str, value: &Value) {
        {
            let mut st = self.st.lock().unwrap();
            let Some(m) = st.installed.get(id).cloned() else { return };
            let Some(s) = m["settings"].as_array().and_then(|a| a.iter().find(|x| x["key"] == key).cloned()) else { return };
            let v = match s["type"].as_str() {
                Some("bool") => Some(json!(!matches!(value, Value::Null | Value::Bool(false)) && value != &json!(0) && value != &json!(""))),
                Some("station") => m["stations"].as_array().filter(|a| a.iter().any(|st| st["id"] == *value)).map(|_| value.clone()),
                Some("range") => value.as_f64().filter(|n| n.is_finite()).map(|n| js_num(nova_core::json::js_round(n).max(s["min"].as_f64().unwrap_or(0.0)).min(s["max"].as_f64().unwrap_or(100.0)))),
                _ => None,
            };
            let Some(v) = v else { return };
            let enabled = Self::enabled(&st, id);
            let mut settings = st.state.get(id).map(|x| x["settings"].clone()).filter(|x| x.is_object()).unwrap_or(json!({}));
            settings[key] = v;
            st.state.insert(id.to_string(), json!({ "enabled": enabled, "settings": settings }));
            let _ = self.ctx.store().save("plugins.json", &Value::Object(st.state.clone()));
        }
        self.changed();
    }
}

#[cfg(test)]
mod radio_tests {
    use super::*;

    #[test]
    fn builtin_tasks_checks_out() {
        let b: Value = serde_json::from_str(TASKS_BUNDLE).unwrap();
        let m = check_bundle(&b).expect("signed and valid");
        assert_eq!(m["id"], TASKS_ID);
        assert_eq!(m["kind"], "tasks");
        let mut bad = b.clone();
        bad["manifest"]["name"] = json!("Tasks!");
        assert!(check_bundle(&bad).is_none());
        // an unknown kind is refused even if it were signed
        let mut other = b.clone();
        other["manifest"]["kind"] = json!("script");
        assert!(check_bundle(&other).is_none());
    }

    #[test]
    fn builtin_radio_checks_out() {
        let b: Value = serde_json::from_str(RADIO_BUNDLE).unwrap();
        let m = check_bundle(&b).expect("signed and valid");
        assert_eq!(m["version"], "1.1.0");
        let st = m["stations"].as_array().unwrap();
        assert_eq!(st.len(), 178);
        let with = st.iter().filter(|s| s.get("surahs").is_some()).count();
        assert_eq!(with, 129);
        // a changed byte breaks the signature
        let mut bad = b.clone();
        bad["manifest"]["stations"][5]["surahs"]["server"] = json!("https://cdn.mp3quran.net/audio/someone-else/r1/");
        assert!(check_bundle(&bad).is_none());
        // a server elsewhere is refused even if it were signed
        assert!(check_surahs(&json!({ "server": "https://evil.example/audio/x/", "list": [1] })).is_none());
        assert!(check_surahs(&json!({ "server": "https://cdn.mp3quran.net/audio/x/r1/", "list": [2, 1] })).is_none());
        assert!(check_surahs(&json!({ "server": "https://cdn.mp3quran.net/audio/x/r1/", "list": [1, 115] })).is_none());
    }
}

#[cfg(test)]
mod focus_timer_manifest_tests {
    use super::*;

    #[test]
    fn focus_timer_manifest_has_strict_bounds() {
        let good: Value = serde_json::from_str(include_str!("../../../plugins/focus-timer/manifest.json")).unwrap();
        assert!(check_focus_timer(&good).is_some());
        let mut bad = good.clone();
        bad["settings"][0]["max"] = json!(10000);
        assert!(check_focus_timer(&bad).is_none());
        let mut duplicate = good.clone();
        duplicate["settings"][1]["key"] = json!("focusMinutes");
        assert!(check_focus_timer(&duplicate).is_none());
    }
}
