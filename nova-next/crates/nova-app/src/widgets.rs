//! Settings › Widgets: the panel for the desktop's widgets (nova-render's desktop.rs keeps
//! widgets.json and draws them). The page shows every widget and ready group as pictures; a
//! click adds one to the desktop or takes it off. A widget's own right-click menu does the same
//! from the desktop, and an open Settings hears about it.
use serde_json::{json, Value};

use nova_render::{desktop, layout};

fn slot_name(s: layout::Slot) -> &'static str {
    match s {
        layout::Slot::Wide => "wide",
        layout::Slot::Small => "small",
    }
}

/// The widgets and ready groups there are, and what's on the desktop.
pub fn get() -> Value {
    let widgets: Vec<Value> = desktop::WIDGETS.iter().map(|(id, ar, en, s)| json!({ "id": id, "ar": ar, "en": en, "slot": slot_name(*s), "soon": !desktop::ready(id) })).collect();
    let groups: Vec<Value> = desktop::GROUPS
        .iter()
        .map(|(id, ar, en, preset, slots)| {
            let rows = layout::preset(preset).map(|p| p.rows.iter().map(|r| r.iter().map(|s| slot_name(*s)).collect::<Vec<_>>()).collect::<Vec<_>>()).unwrap_or_default();
            json!({ "id": id, "ar": ar, "en": en, "rows": rows, "slots": slots, "soon": !desktop::GROUPS_READY })
        })
        .collect();
    let presets: Vec<Value> = layout::PRESETS
        .iter()
        .map(|p| json!({ "id": p.id, "rows": p.rows.iter().map(|r| r.iter().map(|s| slot_name(*s)).collect::<Vec<_>>()).collect::<Vec<_>>() }))
        .collect();
    let mut cfg = desktop::read(&crate::data_dir());
    if let Some(o) = cfg.as_object_mut() {
        o.remove("places");
    }
    json!({ "widgets": widgets, "groups": groups, "presets": presets, "config": cfg })
}

/// The same, with every widget drawn as a picture (when the page opens).
pub fn get_with_previews() -> Value {
    let mut v = get();
    v["previews"] = desktop::previews(&crate::data_dir());
    v
}

/// One change from the page (desktop::apply says which), then the widgets open again as set.
pub fn act(a: &Value) -> Value {
    let dir = crate::data_dir();
    let cfg = desktop::apply(&dir, a);
    nova_core::log::info(&format!("widgets: {a} → {} on the desktop", desktop::shown(&cfg).len()));
    desktop::show(&dir);
    get()
}

/// At start: the widgets, and what their menu does in the app.
pub fn start(app: &std::rc::Rc<std::cell::RefCell<crate::App>>) {
    let weak = std::rc::Rc::downgrade(app);
    desktop::hooks(
        Box::new(|| crate::prefs::open(Some("widgets".into()))),
        Box::new(move || {
            if let Some(a) = weak.upgrade() {
                crate::prefs::send(&a, "widgets", get());
            }
        }),
    );
    desktop::show(&crate::data_dir());
}
