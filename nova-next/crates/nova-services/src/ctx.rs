//! What every service gets: a way to send an event to the island (and Settings), the current
//! features, and the data folder. Services run on their own threads; `emit` is thread-safe.
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use nova_core::settings::Store;
use serde_json::Value;

pub type Emit = Arc<dyn Fn(&str, Value) + Send + Sync>;

#[derive(Clone)]
pub struct Ctx {
    pub emit: Emit,
    pub dir: PathBuf,
    /// The features as the main thread last saw them (it updates this on every change).
    pub features: Arc<Mutex<Value>>,
}

impl Ctx {
    pub fn store(&self) -> Store {
        Store::new(self.dir.clone())
    }
    pub fn feature(&self, k: &str) -> Value {
        self.features.lock().unwrap().get(k).cloned().unwrap_or(Value::Null)
    }
    pub fn on(&self, k: &str) -> bool {
        self.feature(k).as_bool().unwrap_or(false)
    }
    pub fn send(&self, channel: &str, data: Value) {
        (self.emit)(channel, data)
    }
}

/// Milliseconds since 1970 (Date.now()).
pub fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0)
}

/// A sleep that another thread can cut short (clearTimeout + a new setTimeout).
pub struct Wake {
    flag: Mutex<bool>,
    cv: std::sync::Condvar,
}

impl Wake {
    pub fn new() -> Wake {
        Wake { flag: Mutex::new(false), cv: std::sync::Condvar::new() }
    }
    /// Sleeps up to `ms`; true when woken early.
    pub fn sleep(&self, ms: u64) -> bool {
        let g = self.flag.lock().unwrap();
        let (mut g, _) = self.cv.wait_timeout_while(g, std::time::Duration::from_millis(ms), |woken| !*woken).unwrap();
        let woken = *g;
        *g = false;
        woken
    }
    pub fn wake(&self) {
        *self.flag.lock().unwrap() = true;
        self.cv.notify_all();
    }
}

impl Default for Wake {
    fn default() -> Self {
        Wake::new()
    }
}
