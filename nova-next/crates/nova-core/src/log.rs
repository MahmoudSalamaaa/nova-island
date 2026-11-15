//! Error log (log.js): nova.log in the data folder, one line per entry, kept under ~1 MB (the file
//! moves to nova.log.old when it grows past that). Nothing private goes in: no clipboard text, no
//! calendar link, no file contents.
//!   <ISO time> <LEVEL> <message>      levels: ERROR WARN INFO CRASH REJECT
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_BYTES: u64 = 1024 * 1024;

static FILE: Mutex<Option<PathBuf>> = Mutex::new(None);

/// `new Date().toISOString()`: 2026-11-03T01:17:42.123Z
pub fn iso_now() -> String {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    iso(d.as_millis() as i64)
}

/// Milliseconds since 1970 as JavaScript's toISOString.
pub fn iso(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    let (y, m, d) = civil(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z", rem / 3600, rem % 3600 / 60, rem % 60, ms.rem_euclid(1000))
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian (H. Hinnant's algorithm).
pub fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Starts logging to `<dir>/nova.log` and writes the start line.
pub fn init(dir: PathBuf, start_line: &str) {
    let _ = fs::create_dir_all(&dir);
    *FILE.lock().unwrap() = Some(dir.join("nova.log"));
    write("INFO", start_line);
}

pub fn path() -> Option<PathBuf> {
    FILE.lock().unwrap().clone()
}

/// Appends one line; moves a log over 1 MB to nova.log.old first. Failures are ignored, as in 1.7.
pub fn write(level: &str, message: &str) {
    let Some(file) = path() else { return };
    let line = format!("{} {level} {message}\n", iso_now());
    if fs::metadata(&file).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        let mut old = file.clone().into_os_string();
        old.push(".old");
        let _ = fs::rename(&file, old);
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&file) {
        let _ = f.write_all(line.as_bytes());
    }
}

pub fn info(message: &str) {
    write("INFO", message);
}

pub fn error(message: &str) {
    write("ERROR", message);
}

pub fn warn(message: &str) {
    write("WARN", message);
}

/// What "Copy report" puts on the clipboard: versions, state, and the last log lines.
/// `extra` keeps its order (bridge, location, features, vertical, calendar in 1.7).
pub fn report(app_line: &str, engine_line: &str, windows_line: &str, extra: &[(&str, String)]) -> String {
    let tail = path()
        .and_then(|f| fs::read_to_string(f).ok())
        .map(|t| {
            let lines: Vec<&str> = t.split('\n').collect();
            lines[lines.len().saturating_sub(60)..].join("\n")
        })
        .unwrap_or_default();
    let mut out = vec![app_line.to_string(), engine_line.to_string(), windows_line.to_string()];
    out.extend(extra.iter().map(|(k, v)| format!("{k}: {v}")));
    out.push(String::new());
    out.push("--- last log lines ---".into());
    out.push(tail);
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_like_js() {
        assert_eq!(iso(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso(1793650662123), "2026-11-02T20:17:42.123Z");
        assert_eq!(iso(951782400000), "2000-02-29T00:00:00.000Z");
        assert_eq!(iso(-1), "1969-12-31T23:59:59.999Z");
    }
}
