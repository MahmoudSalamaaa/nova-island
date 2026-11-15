//! (1.8.7) The adhan's sound: Settings › Prayer & Dhikr › "Adhan sound" plays it with the prayer
//! alert (pages/views/adhan.js), and the alert lasts as long as the adhan. Two of them: the Fajr
//! adhan ("الصلاة خير من النوم") and the one for the other prayers. Each is Nova's own
//! (pages/sounds/adhan.mp3, adhan-fajr.mp3) unless one is chosen; a chosen one is copied into
//! Nova's data folder as adhan-custom.<ext> / adhan-fajr-custom.<ext>, its original name in the
//! .txt beside it. Without a Fajr file of its own, Fajr uses the other one. The pages play
//! https://nova.island/adhan-audio and /adhan-fajr-audio, which nova-web answers from here.
use std::path::PathBuf;

use serde_json::{json, Value};

const EXTS: &[&str] = &["mp3", "m4a", "ogg", "opus", "wav", "aac"];
/// a few minutes of audio is well under this
const MAX_BYTES: u64 = 30 * 1024 * 1024;

/// "fajr" or the others ("")
fn stem(fajr: bool) -> &'static str {
    if fajr {
        "adhan-fajr-custom"
    } else {
        "adhan-custom"
    }
}

fn custom(fajr: bool) -> Option<PathBuf> {
    let dir = crate::data_dir();
    EXTS.iter().map(|e| dir.join(format!("{}.{e}", stem(fajr)))).find(|p| p.is_file())
}

/// Points nova-web at the chosen files (or back at Nova's own). At start and after a change.
pub fn apply() {
    nova_web::set_adhan_file(false, custom(false));
    nova_web::set_adhan_file(true, custom(true));
}

fn one(fajr: bool) -> Value {
    let name = custom(fajr).map(|_| std::fs::read_to_string(crate::data_dir().join(format!("{}.txt", stem(fajr)))).unwrap_or_else(|_| "adhan".into()));
    let own = nova_assets::get(if fajr { "sounds/adhan-fajr.mp3" } else { "sounds/adhan.mp3" }).is_some();
    json!({ "name": name, "custom": name.is_some(), "hasDefault": own })
}

/// {main: {name, custom, hasDefault}, fajr: {...}} for Settings.
pub fn info() -> Value {
    json!({ "main": one(false), "fajr": one(true) })
}

fn clear(fajr: bool) {
    let dir = crate::data_dir();
    for e in EXTS {
        let _ = std::fs::remove_file(dir.join(format!("{}.{e}", stem(fajr))));
    }
    let _ = std::fs::remove_file(dir.join(format!("{}.txt", stem(fajr))));
}

/// Takes a file the user picked: {ok} or {error}.
pub fn set_file(fajr: bool, path: &str) -> Value {
    let p = PathBuf::from(path);
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if !EXTS.contains(&ext.as_str()) {
        return json!({ "error": "type" });
    }
    if std::fs::metadata(&p).map(|m| m.len()).unwrap_or(u64::MAX) > MAX_BYTES {
        return json!({ "error": "size" });
    }
    clear(fajr);
    let dir = crate::data_dir();
    if std::fs::copy(&p, dir.join(format!("{}.{ext}", stem(fajr)))).is_err() {
        clear(fajr);
        return json!({ "error": "copy" });
    }
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("adhan").to_string();
    let _ = std::fs::write(dir.join(format!("{}.txt", stem(fajr))), name);
    nova_core::log::info(if fajr { "adhan: own Fajr file chosen" } else { "adhan: own file chosen" });
    apply();
    json!({ "ok": true })
}

/// Back to Nova's own adhan.
pub fn reset(fajr: bool) {
    clear(fajr);
    apply();
}
