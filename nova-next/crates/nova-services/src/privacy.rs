//! Microphone / camera in use (privacy.js), like the privacy dots on a Mac or iPhone. Windows
//! records every app's access under CapabilityAccessManager\ConsentStore; an app is using the device
//! right now when its LastUsedTimeStop is 0. Read straight from the registry (1.7 ran reg.exe every
//! 2 s), in the same order reg.exe lists the keys.
use std::sync::{Arc, Mutex};

use serde_json::json;
use windows::{
    core::{HSTRING, PWSTR},
    Win32::System::Registry::*,
};

use crate::ctx::{Ctx, Wake};

const BASE: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore";
const POLL_MS: u64 = 2000;

fn nice(raw: &str) -> String {
    let known = match raw.to_lowercase().as_str() {
        "chrome" => "Chrome", "msedge" => "Edge", "firefox" => "Firefox", "brave" => "Brave", "opera" => "Opera", "vivaldi" => "Vivaldi",
        "teams" | "ms-teams" => "Teams", "zoom" => "Zoom", "discord" => "Discord", "slack" => "Slack", "skype" => "Skype",
        "obs64" | "obs" => "OBS", "telegram" => "Telegram", "whatsapp" => "WhatsApp", "windowscamera" => "Camera",
        "windowssoundrecorder" | "soundrec" => "Sound Recorder", "electron" => "Electron", "screensketch" => "Snipping Tool",
        "loom-recorder-production" => "Loom", "rundll32" => "Windows", "immersivecontrolpanel" => "Settings", "adobe audition cc" => "Adobe Audition",
        _ => "",
    };
    if !known.is_empty() {
        return known.to_string();
    }
    let mut c = raw.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// "C:#Program Files#Google#Chrome#Application#chrome.exe" or "Microsoft.WindowsCamera_8wekyb3d8bbwe"
pub fn app_name(leaf: &str) -> String {
    let raw = if leaf.contains('#') {
        let last = leaf.rsplit('#').next().unwrap_or("");
        if last.to_lowercase().ends_with(".exe") { last[..last.len() - 4].to_string() } else { last.to_string() }
    } else {
        leaf.split('_').next().unwrap_or("").rsplit('.').next().unwrap_or("").to_string()
    };
    nice(&raw)
}

/// Apps whose LastUsedTimeStop is 0 under `key` (depth first, as `reg query /s` lists them).
fn walk(key: HKEY, leaf: &str, out: &mut Vec<String>) {
    unsafe {
        let mut v = 0u64;
        let mut size = 8u32;
        let mut ty = REG_VALUE_TYPE::default();
        if RegQueryValueExW(key, &HSTRING::from("LastUsedTimeStop"), None, Some(&mut ty), Some(&mut v as *mut u64 as *mut u8), Some(&mut size)).is_ok() && ty == REG_QWORD && v == 0 {
            let name = app_name(leaf);
            if !out.contains(&name) {
                out.push(name);
            }
        }
        let mut i = 0;
        loop {
            let mut name = [0u16; 512];
            let mut len = name.len() as u32;
            if RegEnumKeyExW(key, i, Some(PWSTR(name.as_mut_ptr())), &mut len, None, None, None, None).is_err() {
                break;
            }
            i += 1;
            let sub = String::from_utf16_lossy(&name[..len as usize]);
            let mut child = HKEY::default();
            if RegOpenKeyExW(key, &HSTRING::from(sub.as_str()), None, KEY_READ, &mut child).is_ok() {
                walk(child, &sub, out);
                let _ = RegCloseKey(child);
            }
        }
    }
}

pub fn query(device: &str) -> Vec<String> {
    let mut out = Vec::new();
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, &HSTRING::from(format!("{BASE}\\{device}")), None, KEY_READ, &mut key).is_ok() {
            walk(key, device, &mut out);
            let _ = RegCloseKey(key);
        }
    }
    out
}

pub struct Privacy {
    ctx: Ctx,
    enabled: Mutex<bool>,
    last: Mutex<String>,
    wake: Wake,
}

impl Privacy {
    pub fn new(ctx: Ctx) -> Arc<Privacy> {
        let on = ctx.on("privacyDots");
        Arc::new(Privacy { ctx, enabled: Mutex::new(on), last: Mutex::new(String::new()), wake: Wake::new() })
    }

    pub fn start(self: &Arc<Self>) {
        let me = self.clone();
        std::thread::spawn(move || loop {
            if *me.enabled.lock().unwrap() {
                let state = json!({ "mic": query("microphone"), "cam": query("webcam") });
                let key = state.to_string();
                let mut last = me.last.lock().unwrap();
                if key != *last {
                    *last = key;
                    drop(last);
                    me.ctx.send("privacy", state);
                }
                me.wake.sleep(POLL_MS);
            } else {
                me.wake.sleep(60 * 60 * 1000);
            }
        });
    }

    pub fn set_enabled(&self, on: bool) {
        let mut e = self.enabled.lock().unwrap();
        if *e == on {
            return;
        }
        *e = on;
        drop(e);
        self.last.lock().unwrap().clear();
        if !on {
            self.ctx.send("privacy", json!({ "mic": [], "cam": [] }));
        }
        self.wake.wake();
    }
}
