//! Nova's desktop widgets as the user set them, from widgets.json in Nova's data folder. The
//! desktop is the canvas: Settings › Widgets (or a widget's right-click menu) adds and removes
//! them; where they stand is where the user dragged them.
//!   { "on": true,
//!     "groups": [ { "id": "g1", "preset": "wide-2", "slots": ["prayers", "prayer", "hijri"] } ],
//!                 a ready group: a layout (layout.rs) and its widgets, moving as one block
//!     "free": ["moon"],                          widgets on their own
//!     "places": { "g1": [x, y], "moon": [x, y] } }   where they were left (physical pixels)
//! A widget is on the desktop once at most. A new one goes to the first clear place from the
//! screen's top-right corner. The faces read Nova's own files (prayer-cache.json, tasks.json);
//! the touch widgets keep their counts in widgets-state.json.
use std::{cell::RefCell, path::Path, rc::Rc};

use serde_json::{json, Value};

use crate::{date_faces::*, faces::NextPrayer, layout, prayer_faces::*, state::Store, tool_faces::*, widget::{self, Face}};

/// Every widget: its name in widgets.json, its Arabic and English names, its slot.
pub const WIDGETS: &[(&str, &str, &str, layout::Slot)] = &[
    ("prayer", "الصلاة القادمة", "Next prayer", layout::Slot::Small),
    ("prayers", "الصلوات الخمس", "Five prayers", layout::Slot::Wide),
    ("sun", "مسار الشمس", "Sun's path", layout::Slot::Wide),
    ("hijri", "التاريخ الهجري", "Hijri date", layout::Slot::Small),
    ("day", "اسم اليوم", "Day name", layout::Slot::Small),
    ("dates", "اليوم والتاريخان", "Day and dates", layout::Slot::Wide),
    ("month", "التقويم الشهري", "Month", layout::Slot::Small),
    ("tasks", "مهام اليوم", "Today's tasks", layout::Slot::Wide),
    ("tasbih", "السبحة", "Tasbih", layout::Slot::Small),
    ("moon", "طور القمر", "Moon", layout::Slot::Small),
    ("focus", "مؤقت التركيز", "Focus timer", layout::Slot::Small),
    ("water", "زجاجة الماء", "Water bottle", layout::Slot::Small),
    ("pulse", "نبض الجهاز", "PC pulse", layout::Slot::Wide),
];

/// The widgets people can use now (all of them); one left out shows as "coming soon".
pub const READY: &[&str] = &["prayer", "prayers", "sun", "hijri", "day", "dates", "month", "tasks", "tasbih", "moon", "focus", "water", "pulse"];
/// The ready groups can be used too.
pub const GROUPS_READY: bool = true;

pub fn ready(id: &str) -> bool {
    READY.contains(&id)
}

/// Ready groups: (name, Arabic, English, layout, its widgets in the layout's slot order).
pub const GROUPS: &[(&str, &str, &str, &str, &[&str])] = &[
    ("prayer", "الصلاة", "Prayer", "wide-2", &["prayers", "prayer", "hijri"]),
    ("today", "اليوم", "Today", "wide-2", &["dates", "moon", "day"]),
    ("work", "العمل", "Work", "wide-2", &["tasks", "focus", "water"]),
    ("dhikr", "الذكر", "Dhikr", "small-4", &["tasbih", "prayer", "hijri", "moon"]),
    ("board", "لوحة كاملة", "Full board", "2wide-4", &["dates", "prayer", "hijri", "tasks", "tasbih", "focus"]),
    ("pc", "الجهاز", "PC", "wide-2", &["pulse", "water", "focus"]),
];

/// A face by its name.
pub fn face(name: &str, dir: &Path, store: &Store) -> Option<Box<dyn Face>> {
    let json = |file: &str| {
        let p = dir.join(file);
        Box::new(move || std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).unwrap_or_default()) as Box<dyn Fn() -> Value>
    };
    let cache = || json("prayer-cache.json");
    Some(match name {
        "prayer" => Box::new(NextPrayer::new(cache())),
        "prayers" => Box::new(PrayerRow::new(cache())),
        "sun" => Box::new(SunPath::new(cache())),
        "hijri" => Box::new(HijriDate::new(cache())),
        "day" => Box::new(DayName),
        "dates" => Box::new(DayDates::new(cache())),
        "month" => Box::new(Month::new(cache())),
        "tasks" => Box::new(Tasks::new(json("tasks.json"))),
        "tasbih" => Box::new(Tasbih::new(store.clone())),
        "moon" => Box::new(Moon::new(cache())),
        "focus" => Box::new(Focus::new(store.clone())),
        "water" => Box::new(Water::new(store.clone())),
        "pulse" => Box::new(Pulse::new()),
        _ => return None,
    })
}

fn names(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
}
fn known(n: &str) -> bool {
    WIDGETS.iter().any(|w| w.0 == n)
}

/// The settings, with what's missing filled in. Older files are read too: the first
/// prototype's flat places, and a single "layout" with its "slots" (it becomes a group).
pub fn read(dir: &Path) -> Value {
    let mut v: Value = std::fs::read_to_string(dir.join("widgets.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).filter(Value::is_object).unwrap_or_else(|| json!({}));
    if v.get("places").is_none() {
        let flat: serde_json::Map<String, Value> = v.as_object().map(|o| o.iter().filter(|(_, x)| x.is_array()).map(|(k, x)| (k.clone(), x.clone())).collect()).unwrap_or_default();
        v["places"] = Value::Object(flat);
    }
    if v.get("groups").is_none() {
        let mut groups = vec![];
        if let Some(p) = v["layout"].as_str().filter(|p| layout::preset(p).is_some()) {
            groups.push(json!({ "id": "layout", "preset": p, "slots": v["slots"].clone() }));
        }
        v["groups"] = json!(groups);
    }
    if let Some(o) = v.as_object_mut() {
        o.remove("layout");
        o.remove("slots");
        for k in o.keys().cloned().collect::<Vec<_>>() {
            if o[&k].is_array() && k != "groups" && k != "free" {
                o.remove(&k);
            }
        }
    }
    for (k, d) in [("on", json!(false)), ("free", json!([]))] {
        if v.get(k).is_none() {
            v[k] = d;
        }
    }
    // (1.8.7) a field of the wrong kind (a hand edit, a half-written file) is taken as missing
    // instead of a crash later (`free` as {} stopped the app at the next "add")
    if !v["on"].is_boolean() {
        v["on"] = json!(false);
    }
    if !v["free"].is_array() {
        v["free"] = json!([]);
    }
    if !v["places"].is_object() {
        v["places"] = json!({});
    }
    match v["groups"].as_array_mut() {
        Some(gs) => {
            gs.retain(Value::is_object);
            for g in gs.iter_mut().filter(|g| !g["slots"].is_array()) {
                g["slots"] = json!([]);
            }
        }
        None => v["groups"] = json!([]),
    }
    // only what's available now stays on the desktop
    if let Some(a) = v["free"].as_array_mut() {
        a.retain(|x| x.as_str().map(ready).unwrap_or(false));
    }
    if !GROUPS_READY {
        v["groups"] = json!([]);
    }
    v
}

pub fn write(dir: &Path, cfg: &Value) {
    let _ = std::fs::write(dir.join("widgets.json"), serde_json::to_string_pretty(cfg).unwrap_or_default());
}

/// Every widget on the desktop now (in groups or alone).
pub fn shown(cfg: &Value) -> Vec<String> {
    let mut out: Vec<String> = cfg["groups"].as_array().into_iter().flatten().flat_map(|g| names(&g["slots"])).filter(|n| !n.is_empty()).collect();
    out.extend(names(&cfg["free"]));
    out
}

/// Takes a widget off the desktop: out of the free ones, or its group's place left empty (a
/// group left with nothing goes too).
fn take(cfg: &mut Value, id: &str) {
    if let Some(a) = cfg["free"].as_array_mut() {
        a.retain(|x| x != id);
    }
    if let Some(gs) = cfg["groups"].as_array_mut() {
        for g in gs.iter_mut() {
            if let Some(s) = g["slots"].as_array_mut() {
                for x in s.iter_mut() {
                    if x == id {
                        *x = json!("");
                    }
                }
            }
        }
        gs.retain(|g| !names(&g["slots"]).iter().all(String::is_empty));
    }
}

/// One change, from Settings or a widget's menu; the new settings. `{ "on": bool }`,
/// `{ "add": id }`, `{ "remove": id }`, `{ "addGroup": ready-group }`, `{ "removeGroup": gid }`,
/// `{ "ungroup": gid }` (its widgets stay where they are, each on its own), `{ "reset": true }`.
pub fn apply(dir: &Path, act: &Value) -> Value {
    let mut cfg = read(dir);
    if let Some(on) = act["on"].as_bool() {
        cfg["on"] = json!(on);
    }
    if let Some(id) = act["add"].as_str().filter(|n| known(n) && ready(n)) {
        take(&mut cfg, id);
        cfg["free"].as_array_mut().unwrap().push(json!(id));
        if let Some(o) = cfg["places"].as_object_mut() {
            o.remove(id);
        }
        cfg["on"] = json!(true);
    }
    if let Some(id) = act["remove"].as_str() {
        take(&mut cfg, id);
    }
    if let Some(t) = act["addGroup"].as_str().filter(|_| GROUPS_READY).and_then(|t| GROUPS.iter().find(|g| g.0 == t)) {
        for w in t.4 {
            take(&mut cfg, w);
        }
        let n = (1..).find(|i| !cfg["groups"].as_array().unwrap().iter().any(|g| g["id"] == format!("g{i}"))).unwrap();
        let gid = format!("g{n}");
        cfg["groups"].as_array_mut().unwrap().push(json!({ "id": gid, "preset": t.3, "slots": t.4, "from": t.0 }));
        if let Some(o) = cfg["places"].as_object_mut() {
            o.remove(&gid);
        }
        cfg["on"] = json!(true);
    }
    if let Some(gid) = act["removeGroup"].as_str() {
        if let Some(gs) = cfg["groups"].as_array_mut() {
            gs.retain(|g| g["id"] != gid);
        }
    }
    if let Some(gid) = act["ungroup"].as_str() {
        // each keeps the place it has on the screen now
        let here = widget::places();
        let members = cfg["groups"].as_array().unwrap().iter().find(|g| g["id"] == gid).map(|g| names(&g["slots"])).unwrap_or_default();
        if let Some(gs) = cfg["groups"].as_array_mut() {
            gs.retain(|g| g["id"] != gid);
        }
        for m in members.iter().filter(|m| !m.is_empty()) {
            cfg["free"].as_array_mut().unwrap().push(json!(m));
            if let Some((_, x, y)) = here.iter().find(|p| p.0 == *m) {
                cfg["places"][m.as_str()] = json!([x, y]);
            }
        }
    }
    if act["reset"] == true {
        cfg["places"] = json!({});
    }
    write(dir, &cfg);
    cfg
}

thread_local! {
    /// the app's hooks: open Settings › Widgets; the settings changed (from a widget's menu)
    static HOOKS: RefCell<Option<(Rc<dyn Fn()>, Rc<dyn Fn()>)>> = const { RefCell::new(None) };
}

/// What a widget's menu does in the app: opens Settings › Widgets; tells an open Settings.
pub fn hooks(open_settings: Box<dyn Fn()>, changed: Box<dyn Fn()>) {
    HOOKS.with(|h| *h.borrow_mut() = Some((Rc::from(open_settings), Rc::from(changed))));
}

fn features(dir: &Path) -> Value {
    std::fs::read_to_string(dir.join("features.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).unwrap_or_default()
}

/// Settings' language is Arabic; the time format (12 / 24 hours) is set from the same file.
fn arabic(dir: &Path) -> bool {
    let f = features(dir);
    crate::clock::set_24(f["clock24"] == true);
    f["prefsLang"] == "ar"
}

/// Settings › Time format changed: the widgets on the desktop show it at once.
pub fn time_format(clock24: bool) {
    if crate::clock::set_24(clock24) {
        widget::redraw_all();
    }
}

/// Closes the widgets and opens them again as `widgets.json` says (nothing if they're off).
pub fn show(dir: &Path) {
    widget::close_all();
    let cfg = read(dir);
    let ar = arabic(dir);
    widget::menu_text(if ar {
        ["إزالة من سطح المكتب", "فك المجموعة", "إزالة المجموعة", "إعدادات الويدجات…"]
    } else {
        ["Remove from desktop", "Ungroup", "Remove group", "Widget settings…"]
    });
    // the menu's choices: changed and opened again (after the menu's own window message)
    let d = dir.to_path_buf();
    widget::on_action(Box::new(move |id, group, what| {
        let act = match what {
            widget::Action::Remove => json!({ "remove": id }),
            widget::Action::Ungroup => json!({ "ungroup": group.unwrap_or("") }),
            widget::Action::RemoveGroup => json!({ "removeGroup": group.unwrap_or("") }),
            widget::Action::Settings => {
                if let Some((open, _)) = HOOKS.with(|h| h.borrow().clone()) {
                    open();
                }
                return;
            }
        };
        apply(&d, &act);
        show(&d);
        if let Some((_, changed)) = HOOKS.with(|h| h.borrow().clone()) {
            changed();
        }
    }));
    if cfg["on"].as_bool() != Some(true) {
        return;
    }
    let store = Store::open(Some(dir.join("widgets-state.json")));
    let file_dir = dir.to_path_buf();
    // a widget left somewhere: only its place is written (the rest may have changed meanwhile)
    let save = move |id: &str, x: i32, y: i32| {
        let mut now = read(&file_dir);
        now["places"][id] = json!([x, y]);
        write(&file_dir, &now);
    };
    let place = |id: &str| cfg["places"][id].as_array().and_then(|a| Some((a.first()?.as_i64()? as i32, a.get(1)?.as_i64()? as i32)));

    for g in cfg["groups"].as_array().into_iter().flatten() {
        let (Some(gid), Some(p)) = (g["id"].as_str(), g["preset"].as_str().and_then(layout::preset)) else { continue };
        // slot by slot: an empty one ("") leaves its place out, the rest stay in theirs
        let slots = names(&g["slots"]);
        let mut i = 0;
        let mut rows: Vec<Vec<Box<dyn Face>>> = vec![];
        for r in p.rows {
            let row: Vec<Box<dyn Face>> = r
                .iter()
                .filter_map(|_| {
                    i += 1;
                    slots.get(i - 1).and_then(|n| face(n, dir, &store))
                })
                .collect();
            if !row.is_empty() {
                rows.push(row);
            }
        }
        let sizes: Vec<Vec<(f32, f32)>> = rows.iter().map(|r| r.iter().map(|f| f.size()).collect()).collect();
        let (offs, size) = layout::place(&sizes);
        let origin = place(gid).unwrap_or_else(|| widget::free_spot(size));
        let list: Vec<(Box<dyn Face>, (f32, f32))> = rows.into_iter().zip(offs).flat_map(|(r, o)| r.into_iter().zip(o)).collect();
        let s2 = save.clone();
        let _ = widget::open_group(gid, list, origin, Box::new(move |id, x, y| s2(id, x, y)));
    }
    for n in names(&cfg["free"]) {
        let Some(f) = face(&n, dir, &store) else { continue };
        let pos = place(&n).unwrap_or_else(|| widget::free_spot(f.size()));
        let s2 = save.clone();
        let _ = widget::open(f, Some(pos), Box::new(move |id, x, y| s2(id, x, y)));
    }
}

/// Every widget drawn as a picture for Settings, with its size (DIPs):
/// { "dates": { "src": "data:image/png;base64,…", "w": 370, "h": 176 }, … }
pub fn previews(dir: &Path) -> Value {
    crate::clock::set_24(features(dir)["clock24"] == true);
    let store = Store::open(Some(dir.join("widgets-state.json")));
    let scale = (unsafe { windows::Win32::UI::HiDpi::GetDpiForSystem() } as f32 / 96.0).max(1.0);
    let Ok(fonts) = crate::font::Fonts::new() else { return json!({}) };
    let mut out = serde_json::Map::new();
    for (id, ..) in WIDGETS {
        let Some(mut f) = face(id, dir, &store) else { continue };
        let (w, h) = f.size();
        if let Ok(png) = crate::preview::png(f.as_mut(), scale, &fonts) {
            out.insert(id.to_string(), json!({ "src": format!("data:image/png;base64,{}", crate::preview::base64(&png)), "w": w, "h": h }));
        }
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("nova-widgets-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    #[test]
    fn fields_of_the_wrong_kind_are_reset() {
        // (1.8.7) "free": {} stopped the app at the next "add"
        let d = dir("kinds");
        std::fs::write(d.join("widgets.json"), r#"{"on":"yes","free":{},"groups":{},"places":[]}"#).unwrap();
        let v = read(&d);
        assert!(v["on"].is_boolean() && v["free"].is_array() && v["groups"].is_array() && v["places"].is_object());
        let id = WIDGETS.iter().map(|w| w.0).find(|n| ready(n)).unwrap();
        let _ = apply(&d, &json!({ "add": id }));
    }

    #[test]
    fn every_widget_opens_by_name() {
        let store = Store::open(None);
        for (n, _, _, slot) in WIDGETS {
            let f = face(n, Path::new("."), &store).unwrap_or_else(|| panic!("{n}"));
            assert_eq!(f.id(), *n);
            assert_eq!(layout::kind(f.size()), *slot, "{n}");
        }
        // the ready groups' widgets fit their layouts' places
        for g in GROUPS {
            let p = layout::preset(g.3).unwrap();
            let kinds: Vec<layout::Slot> = g.4.iter().map(|n| WIDGETS.iter().find(|w| w.0 == *n).unwrap().3).collect();
            assert_eq!(kinds, p.slots(), "{}", g.0);
        }
    }
    #[test]
    fn older_files_are_read() {
        let d = dir("old");
        std::fs::write(d.join("widgets.json"), r#"{"prayer":[1,2]}"#).unwrap();
        let v = read(&d);
        assert_eq!(v["places"]["prayer"], json!([1, 2]));
        assert_eq!(v["on"], json!(false));
        assert!(v.get("prayer").is_none());
        // a single layout from before becomes a group, its place kept
        std::fs::write(d.join("widgets.json"), r#"{"on":true,"layout":"wide-2","slots":["dates","moon","day"],"free":["pulse"],"places":{"layout":[5,6]}}"#).unwrap();
        let v = read(&d);
        assert_eq!(v["groups"], json!([{ "id": "layout", "preset": "wide-2", "slots": ["dates", "moon", "day"] }]));
        assert_eq!(v["free"], json!(["pulse"]));
        assert_eq!(v["places"]["layout"], json!([5, 6]));
        assert!(v.get("layout").is_none());
    }
    #[test]
    fn every_widget_can_be_added() {
        for (n, ..) in WIDGETS {
            assert!(ready(n), "{n}");
        }
    }
    #[test]
    fn a_widget_is_on_the_desktop_once() {
        let d = dir("once");
        apply(&d, &json!({ "add": "moon" }));
        let v = apply(&d, &json!({ "addGroup": "today" }));
        // the moon went into the group, out of the free ones
        assert_eq!(shown(&v), ["dates", "moon", "day"]);
        assert_eq!(v["on"], json!(true));
        let v = apply(&d, &json!({ "add": "moon" }));
        assert_eq!(v["groups"][0]["slots"], json!(["dates", "", "day"]));
        assert_eq!(v["free"], json!(["moon"]));
        apply(&d, &json!({ "remove": "dates" }));
        let v = apply(&d, &json!({ "remove": "day" }));
        // a group with nothing left goes
        assert_eq!(v["groups"], json!([]));
        let v = apply(&d, &json!({ "addGroup": "work" }));
        let gid = v["groups"][0]["id"].as_str().unwrap().to_string();
        let v = apply(&d, &json!({ "ungroup": gid }));
        assert_eq!(v["groups"], json!([]));
        assert_eq!(v["free"], json!(["moon", "tasks", "focus", "water"]));
    }
}
