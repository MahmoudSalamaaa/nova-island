//! Copied images (nova.js onClipboardSeq / shotToShelf): a copied image shows as a toast with a
//! small thumbnail; with "Save screenshots to Shelf" it goes on the shelf by itself for a while.
//! The Snipping Tool already saves every capture to Pictures\Screenshots: that file goes on the
//! shelf (no second copy). Any other copied image is saved to Pictures\Nova Island first.
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use serde_json::json;

use crate::{
    ctx::{now_ms, Ctx},
    shelf::Shelf,
};

const SNIP_WAIT_MS: u64 = 2500;

fn pictures() -> PathBuf {
    unsafe {
        windows::Win32::UI::Shell::SHGetKnownFolderPath(&windows::Win32::UI::Shell::FOLDERID_Pictures, windows::Win32::UI::Shell::KNOWN_FOLDER_FLAG(0), None)
            .ok()
            .map(|p| {
                let s = p.to_string().unwrap_or_default();
                windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as _));
                PathBuf::from(s)
            })
            .unwrap_or_default()
    }
}

pub struct Screenshots {
    ctx: Ctx,
    shelf: Arc<Shelf>,
    last: Mutex<Option<(u32, u32, Vec<u8>)>>,
}

impl Screenshots {
    pub fn new(ctx: Ctx, shelf: Arc<Shelf>) -> Arc<Screenshots> {
        Arc::new(Screenshots { ctx, shelf, last: Mutex::new(None) })
    }

    /// After a clipboard change (not a private one).
    pub fn clipboard_changed(self: &Arc<Self>) {
        if !self.ctx.on("screenshotToast") {
            return;
        }
        let Some((w, h, px)) = nova_sys::clip::read_image() else { return };
        let thumb = nova_sys::icons::bgra_png(w, h, &px, Some(72)).map(|b| format!("data:image/png;base64,{}", nova_sys::media::base64(&b))).unwrap_or_default();
        *self.last.lock().unwrap() = Some((w, h, px));
        let auto = self.ctx.on("shotAutoShelf") && self.ctx.on("tabShelf");
        self.ctx.send("screenshot", json!({ "thumb": thumb, "width": w, "height": h, "auto": auto }));
        if auto {
            let minutes = self.ctx.feature("shotShelfMinutes").as_f64().unwrap_or(20.0);
            self.to_shelf(now_ms() + minutes * 60.0 * 1000.0);
        }
    }

    /// The newest picture the Snipping Tool saved since `since` (it may take a moment).
    fn windows_screenshot(since: f64) -> Option<PathBuf> {
        let dir = pictures().join("Screenshots");
        let mut waited = 0;
        while waited <= SNIP_WAIT_MS {
            let fresh = std::fs::read_dir(&dir).ok().and_then(|r| {
                r.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| matches!(x.to_string_lossy().to_lowercase().as_str(), "png" | "jpg" | "jpeg")))
                    .filter_map(|p| std::fs::metadata(&p).ok().map(|m| (p, m)))
                    .filter(|(_, m)| m.len() > 0 && m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).is_some_and(|d| d.as_millis() as f64 >= since - 1500.0))
                    .max_by_key(|(_, m)| m.modified().ok())
            });
            if let Some((p, _)) = fresh {
                return Some(p);
            }
            std::thread::sleep(Duration::from_millis(250));
            waited += 250;
        }
        None
    }

    /// "Add to Shelf", or the automatic one (until > 0: leaves the shelf then).
    pub fn to_shelf(self: &Arc<Self>, until: f64) {
        let me = self.clone();
        // (1.8.7) the picture and its time as they are now: a newer copy before the thread runs
        // isn't saved in this one's place
        let Some((w, h, px)) = me.last.lock().unwrap().clone() else { return };
        let copied_at = now_ms();
        std::thread::spawn(move || {
            let file = if until > 0.0 { Self::windows_screenshot(copied_at) } else { None };
            let file = match file {
                Some(f) => f,
                None => {
                    let dir = pictures().join("Nova Island");
                    let zone = nova_prayer::icu::default_zone();
                    let Some(t) = nova_prayer::wall(&zone, copied_at) else { return };
                    let base = format!("Screenshot {}-{:02}-{:02} {:02}{:02}{:02}", t.year, t.month, t.day, t.hour, t.minute, t.second);
                    let Some(png) = nova_sys::icons::bgra_png(w, h, &px, None) else { return };
                    // (1.8.7) two in the same second: "… (2).png", never over the first one
                    let saved = std::fs::create_dir_all(&dir).ok().and_then(|_| {
                        (1..100).find_map(|i| {
                            let path = dir.join(if i == 1 { format!("{base}.png") } else { format!("{base} ({i}).png") });
                            use std::io::Write;
                            match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
                                Ok(mut f) => Some(f.write_all(&png).map(|_| path)),
                                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => None,
                                Err(e) => Some(Err(e)),
                            }
                        })
                    });
                    let Some(Ok(path)) = saved else {
                        nova_core::log::error("[screenshot] can't save the picture");
                        return;
                    };
                    path
                }
            };
            me.shelf.add(&[file.to_string_lossy().into_owned()], until);
            if until == 0.0 {
                let name = Path::new(&file).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                me.ctx.send("shot-saved", json!({ "name": name }));
            }
        });
    }
}
