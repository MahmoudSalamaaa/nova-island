//! Tasks (1.8.3, the built-in Tasks plugin): a to-do list with due times, reminders and a tag
//! (low, medium, high, urgent), kept in tasks.json in the data folder. A reminder shows on the
//! island at its time ("task-due"); every change goes to the island and the Tasks window ("tasks").
//! Everything a page sends is checked here: lengths, the tag, the reminder, real times.
use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::{json, Map, Value};

use crate::ctx::{now_ms, Ctx, Wake};

const FILE: &str = "tasks.json";
const MAX_TASKS: usize = 500;
const MAX_TITLE: usize = 200;
const MAX_NOTES: usize = 2000;
pub const TAGS: &[&str] = &["low", "medium", "high", "urgent"];
/// minutes before the due time; 0 is at the due time
pub const REMINDERS: &[i64] = &[0, 5, 10, 15, 30, 60, 120, 1440];
/// a reminder more than this late (the PC was off) is dropped, not shown
const STALE_MS: f64 = 12.0 * 60.0 * 60.0 * 1000.0;
const SNOOZE_MIN: &[i64] = &[5, 10, 15, 30, 60];

pub struct Tasks {
    ctx: Ctx,
    list: Mutex<Vec<Value>>,
    wake: Wake,
    /// the plugin is installed and on (the app sets it from the plugin list)
    active: Mutex<bool>,
}

fn text(v: &Value, max: usize) -> String {
    let s = v.as_str().unwrap_or("").replace(['\r', '\0'], "");
    let s = s.trim();
    s.chars().take(max).collect()
}

/// A time in ms, between 2000 and 2200, or None.
fn time(v: &Value) -> Option<f64> {
    v.as_f64().filter(|t| t.is_finite() && *t > 946_684_800_000.0 && *t < 7_258_118_400_000.0).map(|t| t.round())
}

impl Tasks {
    pub fn new(ctx: Ctx) -> Arc<Tasks> {
        let list = ctx.store().load(FILE).and_then(|v| v.as_array().cloned()).unwrap_or_default();
        let list = list.iter().filter_map(|t| Self::clean(t, None)).take(MAX_TASKS).collect();
        Arc::new(Tasks { ctx, list: Mutex::new(list), wake: Wake::new(), active: Mutex::new(false) })
    }

    /// A task as stored: only known fields, checked. `old` is the task being edited (its id, its
    /// creation time and its reminder state carry over).
    fn clean(v: &Value, old: Option<&Value>) -> Option<Value> {
        let title = text(&v["title"], MAX_TITLE);
        if title.is_empty() {
            return None;
        }
        let id = old.and_then(|o| o["id"].as_str()).or(v["id"].as_str()).filter(|s| (1..=40).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')).map(str::to_string).unwrap_or_else(|| format!("t{}", now_ms() as u64));
        let tag = v["tag"].as_str().filter(|t| TAGS.contains(t)).unwrap_or("medium");
        let due = time(&v["due"]);
        let remind = v["remind"].as_i64().filter(|m| REMINDERS.contains(m)).filter(|_| due.is_some());
        let done = v["done"].as_bool().unwrap_or(false);
        let created = old.and_then(|o| time(&o["created"])).or(time(&v["created"])).unwrap_or_else(now_ms);
        // the reminder fires again when its time moves
        let same_time = old.is_some_and(|o| time(&o["due"]) == due && o["remind"].as_i64() == remind);
        let reminded = if old.is_some() { same_time && old.is_some_and(|o| o["reminded"] == true) } else { v["reminded"] == true };
        let snooze = if old.is_some() && !same_time { None } else { time(&v["snooze"]).or(old.and_then(|o| time(&o["snooze"]))) };
        let mut t = Map::new();
        t.insert("id".into(), json!(id));
        t.insert("title".into(), json!(title));
        t.insert("notes".into(), json!(text(&v["notes"], MAX_NOTES)));
        t.insert("tag".into(), json!(tag));
        t.insert("due".into(), due.map_or(Value::Null, |d| json!(d)));
        t.insert("remind".into(), remind.map_or(Value::Null, |m| json!(m)));
        t.insert("done".into(), json!(done));
        t.insert("doneAt".into(), if done { time(&v["doneAt"]).or(old.and_then(|o| time(&o["doneAt"]))).map_or(json!(now_ms()), |d| json!(d)) } else { Value::Null });
        t.insert("created".into(), json!(created));
        t.insert("reminded".into(), json!(reminded));
        t.insert("snooze".into(), snooze.map_or(Value::Null, |s| json!(s)));
        Some(Value::Object(t))
    }

    /// (1.8.7) Writes the changed list while it's still locked (two changes can't write their
    /// copies out of order), then tells the pages. When the disk says no, the change is undone
    /// (`before`) and false comes back: the page is never told a task was kept when it wasn't.
    fn commit(&self, mut list: MutexGuard<Vec<Value>>, before: Vec<Value>) -> bool {
        // (the store logs why)
        if self.ctx.store().save(FILE, &json!(*list)).is_err() {
            *list = before;
            return false;
        }
        let snapshot = list.clone();
        drop(list);
        self.ctx.send("tasks", json!(snapshot));
        self.wake.wake();
        true
    }

    pub fn list(&self) -> Value {
        json!(*self.list.lock().unwrap())
    }

    /// A new task; its stored form (with its id), or Null when it isn't valid.
    pub fn add(&self, v: &Value) -> Value {
        let Some(t) = Self::clean(v, None) else { return Value::Null };
        let mut list = self.list.lock().unwrap();
        if list.len() >= MAX_TASKS {
            return Value::Null;
        }
        let before = list.clone();
        // a fresh id, even for two tasks added in the same millisecond
        let mut t = t;
        while list.iter().any(|x| x["id"] == t["id"]) {
            t["id"] = json!(format!("{}x", t["id"].as_str().unwrap_or("t")));
        }
        list.insert(0, t.clone());
        if !self.commit(list, before) {
            return Value::Null;
        }
        t
    }

    /// Changes task `id` with the fields in `patch`.
    pub fn update(&self, id: &str, patch: &Value) -> bool {
        let mut list = self.list.lock().unwrap();
        let Some(i) = list.iter().position(|t| t["id"].as_str() == Some(id)) else { return false };
        let mut merged = list[i].clone();
        if let (Some(m), Some(p)) = (merged.as_object_mut(), patch.as_object()) {
            for (k, v) in p {
                if ["title", "notes", "tag", "due", "remind", "done"].contains(&k.as_str()) {
                    m.insert(k.clone(), v.clone());
                }
            }
            if p.get("done") == Some(&json!(true)) && list[i]["done"] != true {
                m.insert("doneAt".into(), json!(now_ms()));
            }
        }
        let Some(t) = Self::clean(&merged, Some(&list[i])) else { return false };
        let before = list.clone();
        list[i] = t;
        self.commit(list, before)
    }

    pub fn remove(&self, id: &str) -> bool {
        let mut list = self.list.lock().unwrap();
        let before = list.clone();
        list.retain(|t| t["id"].as_str() != Some(id));
        if list.len() == before.len() {
            return false;
        }
        self.commit(list, before)
    }

    /// Clears the finished tasks.
    pub fn clear_done(&self) {
        let mut list = self.list.lock().unwrap();
        let before = list.clone();
        list.retain(|t| t["done"] != true);
        self.commit(list, before);
    }

    /// Shows the reminder again in `minutes` (5, 10, 15, 30 or 60).
    pub fn snooze(&self, id: &str, minutes: i64) -> bool {
        if !SNOOZE_MIN.contains(&minutes) {
            return false;
        }
        let mut list = self.list.lock().unwrap();
        let before = list.clone();
        let Some(t) = list.iter_mut().find(|t| t["id"].as_str() == Some(id)) else { return false };
        t["snooze"] = json!((now_ms() + minutes as f64 * 60_000.0).round());
        self.commit(list, before)
    }

    /// The plugin is on or off (from the plugin list): reminders only while it's on.
    pub fn set_active(&self, on: bool) {
        *self.active.lock().unwrap() = on;
        self.wake.wake();
    }

    pub fn start(self: &Arc<Self>) {
        let me = self.clone();
        std::thread::spawn(move || loop {
            let next = me.check();
            me.wake.sleep(next.clamp(1_000, 60_000));
        });
    }

    /// Shows the reminders that are due; how long until the next one (ms).
    fn check(&self) -> u64 {
        if !*self.active.lock().unwrap() {
            return 60_000;
        }
        let now = now_ms();
        let mut next = f64::INFINITY;
        let mut due_now = Vec::new();
        let mut changed = false;
        {
            let mut list = self.list.lock().unwrap();
            for t in list.iter_mut() {
                if t["done"] == true {
                    continue;
                }
                let at = match (time(&t["snooze"]), time(&t["due"]), t["remind"].as_i64()) {
                    (Some(s), _, _) => s,
                    (None, Some(d), Some(m)) if t["reminded"] != true => d - m as f64 * 60_000.0,
                    _ => continue,
                };
                if at > now {
                    next = next.min(at - now);
                    continue;
                }
                t["reminded"] = json!(true);
                t["snooze"] = Value::Null;
                changed = true;
                if now - at <= STALE_MS {
                    due_now.push(t.clone());
                }
            }
            if changed {
                let snapshot = list.clone();
                // a reminder already shown stays shown even if the disk said no (no repeats)
                let _ = self.ctx.store().save(FILE, &json!(snapshot));
                self.ctx.send("tasks", json!(snapshot));
            }
        }
        for t in due_now {
            self.ctx.send("task-due", t);
        }
        if next.is_finite() { next as u64 + 50 } else { 60_000 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_task_is_checked() {
        let t = Tasks::clean(&json!({ "title": "  Call the bank  ", "tag": "urgent", "due": 1_800_000_000_000.0_f64, "remind": 15, "evil": "<script>" }), None).unwrap();
        assert_eq!(t["title"], "Call the bank");
        assert_eq!(t["tag"], "urgent");
        assert_eq!(t["remind"], 15);
        assert!(t.get("evil").is_none());
        // a wrong tag becomes medium, an unknown reminder none, no reminder without a time
        let t = Tasks::clean(&json!({ "title": "x", "tag": "max", "remind": 7, "due": 1_800_000_000_000.0_f64 }), None).unwrap();
        assert_eq!(t["tag"], "medium");
        assert_eq!(t["remind"], Value::Null);
        let t = Tasks::clean(&json!({ "title": "x", "remind": 15 }), None).unwrap();
        assert_eq!(t["remind"], Value::Null);
        // no title, no task; a long one is cut
        assert!(Tasks::clean(&json!({ "title": "   " }), None).is_none());
        let long = "a".repeat(500);
        assert_eq!(Tasks::clean(&json!({ "title": long }), None).unwrap()["title"].as_str().unwrap().len(), MAX_TITLE);
        // a date outside 2000..2200 is no date
        assert_eq!(Tasks::clean(&json!({ "title": "x", "due": 5 }), None).unwrap()["due"], Value::Null);
    }

    #[test]
    fn moving_the_time_rearms_the_reminder() {
        let old = Tasks::clean(&json!({ "title": "x", "due": 1_800_000_000_000.0_f64, "remind": 0, "reminded": true }), None).unwrap();
        let same = Tasks::clean(&json!({ "title": "y", "due": 1_800_000_000_000.0_f64, "remind": 0 }), Some(&old)).unwrap();
        assert_eq!(same["reminded"], true);
        assert_eq!(same["id"], old["id"]);
        let moved = Tasks::clean(&json!({ "title": "y", "due": 1_800_000_600_000.0_f64, "remind": 0 }), Some(&old)).unwrap();
        assert_eq!(moved["reminded"], false);
    }
}
