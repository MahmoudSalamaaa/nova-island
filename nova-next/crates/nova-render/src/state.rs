//! What the touch widgets remember (the tasbih's count, today's glasses of water): one small JSON
//! file, written on each change.
use std::{cell::RefCell, path::PathBuf, rc::Rc};

use serde_json::Value;

#[derive(Clone)]
pub struct Store(Rc<RefCell<(Option<PathBuf>, Value)>>);

impl Store {
    /// From a file (None: kept in memory only).
    pub fn open(path: Option<PathBuf>) -> Store {
        let v = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).filter(Value::is_object).unwrap_or_else(|| serde_json::json!({}));
        Store(Rc::new(RefCell::new((path, v))))
    }
    pub fn get(&self, key: &str) -> Value {
        self.0.borrow().1.get(key).cloned().unwrap_or(Value::Null)
    }
    pub fn set(&self, key: &str, v: Value) {
        let mut s = self.0.borrow_mut();
        s.1[key] = v;
        if let Some(p) = &s.0 {
            let _ = std::fs::write(p, s.1.to_string());
        }
    }
}
