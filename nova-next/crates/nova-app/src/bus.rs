// Messages from the service threads to the window thread: an event for the pages, or the answer to
// an invoke a worker thread computed. Each one wakes the window (WM_BUS), which hands it on.
use std::sync::{
    mpsc::{channel, Receiver, Sender},
    Arc, Mutex,
};

use serde_json::Value;
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::WindowsAndMessaging::{PostMessageW, WM_APP},
};

pub const WM_BUS: u32 = WM_APP + 3;

pub enum Msg {
    Event(String, Value),
    Resolve(u64, Value),
    /// work that has to happen on the window thread (opening a window)
    Ui(Box<dyn FnOnce(&std::rc::Rc<std::cell::RefCell<crate::App>>) + Send>),
}

#[derive(Clone)]
pub struct Bus {
    tx: Arc<Mutex<Sender<Msg>>>,
    hwnd: isize,
}

impl Bus {
    pub fn new(hwnd: HWND) -> (Bus, Receiver<Msg>) {
        let (tx, rx) = channel();
        (Bus { tx: Arc::new(Mutex::new(tx)), hwnd: hwnd.0 as isize }, rx)
    }

    fn post(&self, m: Msg) {
        if self.tx.lock().unwrap().send(m).is_ok() {
            unsafe {
                let _ = PostMessageW(Some(HWND(self.hwnd as *mut _)), WM_BUS, WPARAM(0), LPARAM(0));
            }
        }
    }

    pub fn event(&self, channel: &str, data: Value) {
        self.post(Msg::Event(channel.to_string(), data));
    }

    pub fn resolve(&self, id: u64, v: Value) {
        self.post(Msg::Resolve(id, v));
    }

    pub fn ui(&self, f: impl FnOnce(&std::rc::Rc<std::cell::RefCell<crate::App>>) + Send + 'static) {
        self.post(Msg::Ui(Box::new(f)));
    }

    /// The thread-safe `emit` services use.
    pub fn emitter(&self) -> nova_services::ctx::Emit {
        let me = self.clone();
        Arc::new(move |ch: &str, v: Value| me.event(ch, v))
    }
}

/// (1.8.7) A web link Nova itself read off the clipboard (smart clipboard): http too, as its
/// "Open" button offers for one. Only a plain http(s)://host... address, nothing else.
pub fn open_web_link(url: &str) {
    let l = url.to_ascii_lowercase();
    let rest = l.strip_prefix("https://").or_else(|| l.strip_prefix("http://")).unwrap_or("");
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() || host.contains('@') || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return;
    }
    unsafe {
        windows::Win32::UI::Shell::ShellExecuteW(None, &windows::core::HSTRING::from("open"), &windows::core::HSTRING::from(url), None, None, windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL);
    }
}

/// Opens an https link (or a Windows settings page) in its default app, like shell.openExternal.
pub fn open_external(url: &str) {
    let ok = url.to_ascii_lowercase().starts_with("https://") || url.starts_with("ms-settings:");
    if !ok {
        return;
    }
    unsafe {
        windows::Win32::UI::Shell::ShellExecuteW(None, &windows::core::HSTRING::from("open"), &windows::core::HSTRING::from(url), None, None, windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL);
    }
}
