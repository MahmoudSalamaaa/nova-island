//! Small JSON store in the app's data folder (settings.js): %APPDATA%\dynamic-nova, the folder 1.7
//! uses, so 1.8 keeps every setting.
use std::{fs, path::PathBuf};

use serde_json::Value;

use crate::json;

/// %APPDATA%\dynamic-nova (nova.js sets userData to this before anything else loads).
pub fn dir() -> PathBuf {
    PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("dynamic-nova")
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Store {
        Store { dir }
    }

    pub fn file(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// The parsed file, or None when it's missing or not JSON (settings.js returns its fallback).
    pub fn load(&self, name: &str) -> Option<Value> {
        let text = fs::read_to_string(self.file(name)).ok()?;
        // no BOM stripping: JSON.parse fails on one, so 1.7 falls back to the default there too
        let v = serde_json::from_str(&text).ok();
        if v.is_none() && !text.trim().is_empty() {
            // (1.8.7) the next save replaces it: a copy is kept as <name>.bad first
            let _ = fs::copy(self.file(name), self.file(&format!("{name}.bad")));
            crate::log::error(&format!("[settings] {name} is not JSON; a copy is in {name}.bad"));
        }
        v
    }

    /// Writes `JSON.stringify(data, null, 2)`, creating the folder if needed. Errors go to the log
    /// line 1.7 writes ("[settings] <message>") and back to the caller.
    ///
    /// (1.8.7) Through `<name>.tmp` and a rename, so the file is the old one or the new one, never
    /// half of one (the app closed, or the PC lost power, in the middle of a write).
    pub fn save(&self, name: &str, data: &Value) -> Result<(), String> {
        let r = fs::create_dir_all(&self.dir).and_then(|_| {
            let (file, tmp) = (self.file(name), self.file(&format!("{name}.tmp")));
            {
                use std::io::Write;
                let mut f = fs::File::create(&tmp)?;
                f.write_all(json::pretty(data).as_bytes())?;
                f.sync_all()?;
            }
            // an antivirus scanning the new file can hold it a moment
            let mut last = Ok(());
            for i in 0..4 {
                last = fs::rename(&tmp, &file);
                if last.is_ok() {
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_millis(15 << i));
            }
            let _ = fs::remove_file(&tmp);
            last
        });
        r.map_err(|e| {
            let m = format!("[settings] {name}: {e}");
            crate::log::error(&m);
            m
        })
    }
}
