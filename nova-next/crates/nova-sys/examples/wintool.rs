//! Window checks for the regression tests (PowerShell drives Explorer and Photos; this reads the
//! island's window and presses a key only into the app named):
//!   wintool island <pid>             visible=… topmost=… above-front=… (the island of that process)
//!   wintool front                    the process in front
//!   wintool key <F11|ESC|ALTF4> <name>   the key, only if <name> is in front (else "skip")
//!   wintool untop <pid>              takes the island's always-on-top away (what the bug did)
//!   wintool click <x> <y> [dx dy]    a click (or a drag by dx, dy) at a screen point, only on a
//!                                    Nova widget (else "skip")
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::HWND,
        UI::{
            Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_ESCAPE, VK_F11, VK_F4, VK_MENU},
            WindowsAndMessaging::{FindWindowExW, GetForegroundWindow, GetWindow, GetWindowLongPtrW, GetWindowThreadProcessId, IsWindowVisible, SetWindowPos, GWL_EXSTYLE, GW_HWNDNEXT, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WS_EX_TOPMOST},
        },
    },
};

fn island_of(pid: u32) -> Option<HWND> {
    let mut after = None;
    loop {
        let h = unsafe { FindWindowExW(None, after, w!("NovaIsland"), PCWSTR::null()) }.ok().filter(|h| !h.is_invalid())?;
        let mut p = 0u32;
        unsafe { GetWindowThreadProcessId(h, Some(&mut p)) };
        if p == pid {
            return Some(h);
        }
        after = Some(h);
    }
}

fn front_name() -> String {
    let mut p = 0u32;
    unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut p)) };
    nova_sys::win::process_name(p)
}

fn press(keys: &[VIRTUAL_KEY]) {
    let ev = |vk: VIRTUAL_KEY, up: bool| INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() }, ..Default::default() } } };
    let mut list: Vec<INPUT> = keys.iter().map(|k| ev(*k, false)).collect();
    list.extend(keys.iter().rev().map(|k| ev(*k, true)));
    unsafe { SendInput(&list, std::mem::size_of::<INPUT>() as i32) };
}

/// A press at (x, y), a move by (dx, dy), a release; only where a Nova widget is.
fn click(x: i32, y: i32, dx: i32, dy: i32) -> bool {
    use windows::Win32::{
        Foundation::POINT,
        UI::{
            Input::KeyboardAndMouse::{INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT},
            WindowsAndMessaging::{GetClassNameW, SetCursorPos, WindowFromPoint},
        },
    };
    unsafe {
        let h = WindowFromPoint(POINT { x, y });
        let mut buf = [0u16; 64];
        let n = GetClassNameW(h, &mut buf) as usize;
        if String::from_utf16_lossy(&buf[..n]) != "NovaWidget" {
            return false;
        }
        let ev = |f| INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dwFlags: f, ..Default::default() } } };
        let _ = SetCursorPos(x, y);
        SendInput(&[ev(MOUSEEVENTF_LEFTDOWN)], std::mem::size_of::<INPUT>() as i32);
        std::thread::sleep(std::time::Duration::from_millis(60));
        for i in 1..=10 {
            let _ = SetCursorPos(x + dx * i / 10, y + dy * i / 10);
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        SendInput(&[ev(MOUSEEVENTF_LEFTUP)], std::mem::size_of::<INPUT>() as i32);
        true
    }
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a.first().map(String::as_str) {
        Some("island") => {
            let Some(h) = a.get(1).and_then(|p| p.parse().ok()).and_then(island_of) else { return println!("no island") };
            let fg = unsafe { GetForegroundWindow() };
            let mut above = false;
            let mut n = h;
            while let Ok(next) = unsafe { GetWindow(n, GW_HWNDNEXT) } {
                if next.is_invalid() {
                    break;
                }
                if next == fg {
                    above = true;
                    break;
                }
                n = next;
            }
            let top = unsafe { GetWindowLongPtrW(h, GWL_EXSTYLE) } as u32 & WS_EX_TOPMOST.0 != 0;
            println!("visible={} topmost={} above-front={}", unsafe { IsWindowVisible(h).as_bool() }, top, above);
        }
        Some("front") => {
            let h = unsafe { GetForegroundWindow() };
            let mut r = windows::Win32::Foundation::RECT::default();
            let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowRect(h, &mut r) };
            println!("{} ({},{},{},{}) full-screen-rule={}", front_name(), r.left, r.top, r.right, r.bottom, nova_sys::win::is_fullscreen());
        }
        Some("key") => {
            let (Some(k), Some(want)) = (a.get(1), a.get(2)) else { return };
            let front = front_name();
            if !front.to_ascii_lowercase().contains(&want.to_ascii_lowercase()) {
                return println!("skip (front is {front})");
            }
            match k.as_str() {
                "F11" => press(&[VK_F11]),
                "ESC" => press(&[VK_ESCAPE]),
                "ALTF4" => press(&[VK_MENU, VK_F4]),
                _ => return println!("unknown key"),
            }
            println!("sent {k} to {front}");
        }
        Some(c @ ("hide" | "show")) => {
            if let Some(h) = a.get(1).and_then(|p| p.parse().ok()).and_then(island_of) {
                let cmd = if c == "hide" { windows::Win32::UI::WindowsAndMessaging::SW_HIDE } else { windows::Win32::UI::WindowsAndMessaging::SW_SHOWNOACTIVATE };
                let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::ShowWindow(h, cmd) };
                println!("{c}");
            }
        }
        Some("untop") => {
            if let Some(h) = a.get(1).and_then(|p| p.parse().ok()).and_then(island_of) {
                let _ = unsafe { SetWindowPos(h, Some(HWND(-2isize as _)), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) };
                println!("untopped");
            }
        }
        // the Nova widgets' window regions (the rounded cut)
        Some("rgn") => unsafe {
            use windows::Win32::{Foundation::RECT, Graphics::Gdi::{CreateRectRgn, GetRgnBox, GetWindowRgn}};
            let mut after = None;
            while let Ok(h) = FindWindowExW(None, after, w!("NovaWidget"), PCWSTR::null()) {
                if h.is_invalid() {
                    break;
                }
                let r = CreateRectRgn(0, 0, 0, 0);
                let kind = GetWindowRgn(h, r);
                let mut b = RECT::default();
                GetRgnBox(r, &mut b);
                println!("widget {:?}: region kind {} box {:?}", h.0, kind.0, (b.left, b.top, b.right, b.bottom));
                after = Some(h);
            }
        },
        Some("click") => {
            let n: Vec<i32> = a[1..].iter().filter_map(|v| v.parse().ok()).collect();
            if n.len() < 2 {
                return println!("wintool click <x> <y> [dx dy]");
            }
            let ok = click(n[0], n[1], *n.get(2).unwrap_or(&0), *n.get(3).unwrap_or(&0));
            println!("{}", if ok { "clicked" } else { "skip" });
        }
        _ => println!("wintool island <pid> | front | key <F11|ESC|ALTF4> <name> | untop <pid>"),
    }
}
