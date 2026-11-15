// The Tasks window (1.8.3, the built-in Tasks plugin): the whole to-do list, to add, edit, finish,
// search and sort tasks. The list itself lives in the Tasks service (nova-services/tasks.rs); this
// passes the window's requests to it. The island shows the next tasks and the reminders.
use std::{cell::RefCell, rc::Rc};

use serde_json::{json, Value};

use crate::{prefs::Kind, App};

/// Whether the Tasks plugin is installed and on.
pub fn active(a: &Rc<RefCell<App>>) -> bool {
    let list = a.borrow().plugins.active();
    list.as_array().is_some_and(|l| l.iter().any(|p| p["kind"] == "tasks"))
}

/// The plugin list changed: reminders follow whether Tasks is on, and the window closes with it.
pub fn plugins_changed(a: &Rc<RefCell<App>>) {
    let on = active(a);
    a.borrow().tasks.set_active(on);
    if !on {
        crate::prefs::close_kind(Kind::Tasks);
    }
}

fn id_of(v: &Value) -> Option<&str> {
    v.as_str().filter(|s| (1..=40).contains(&s.len()))
}

/// Everything the window needs when it opens.
fn data(a: &Rc<RefCell<App>>) -> Value {
    let s = a.borrow();
    json!({
        "tasks": s.tasks.list(),
        "tags": nova_services::tasks::TAGS,
        "reminders": nova_services::tasks::REMINDERS,
        "lang": s.core.features.get().get("prefsLang").cloned().unwrap_or(json!("en")),
        "clock24": s.core.features.bool("clock24"),
    })
}

/// Messages from the window (tasks/preload.js).
pub fn channel(a: &Rc<RefCell<App>>, channel: &str, args: &[Value]) -> Option<Value> {
    let arg0 = args.first().cloned().unwrap_or(Value::Null);
    let tasks = a.borrow().tasks.clone();
    let v = match channel {
        "tasks-data" => data(a),
        "tasks-add" => tasks.add(&arg0),
        "tasks-update" => json!(id_of(&arg0).is_some_and(|id| tasks.update(id, args.get(1).unwrap_or(&Value::Null)))),
        "tasks-remove" => json!(id_of(&arg0).is_some_and(|id| tasks.remove(id))),
        "tasks-clear-done" => {
            tasks.clear_done();
            Value::Null
        }
        "tasks-window" => {
            match arg0.as_str() {
                Some("close") => crate::prefs::close_kind(Kind::Tasks),
                Some("minimize") => crate::prefs::minimize_kind(Kind::Tasks),
                _ => {}
            }
            Value::Null
        }
        _ => return None,
    };
    Some(v)
}

/// The list changed: the window shows it too.
pub fn to_window(a: &Rc<RefCell<App>>, list: Value) {
    crate::prefs::send_kind(a, Kind::Tasks, "tasks", list);
}
