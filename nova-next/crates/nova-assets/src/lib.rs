//! The app's pages (island, Settings, mini player), embedded unchanged from the repo root.
include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// A page file by its path under https://nova.island/ ("index.html", "views/apps.js", ...).
pub fn get(path: &str) -> Option<(&'static [u8], &'static str)> {
    FILES.iter().find(|(p, _, _)| *p == path).map(|(_, bytes, mime)| (*bytes, *mime))
}
