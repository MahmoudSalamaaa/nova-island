// Reads the tab rectangles of each visible Chrome / Edge window (read only) and times the calls.
use windows::{core::BOOL, Win32::{Foundation::*, System::Com::*, UI::WindowsAndMessaging::*}};

unsafe extern "system" fn each(h: HWND, l: LPARAM) -> BOOL {
    let list = &mut *(l.0 as *mut Vec<HWND>);
    let c = nova_sys::win::class_of(h);
    if IsWindowVisible(h).as_bool() && (c == "Chrome_WidgetWin_1" || c == "MozillaWindowClass") {
        list.push(h);
    }
    true.into()
}

fn main() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let mut wins: Vec<HWND> = Vec::new();
        let _ = EnumWindows(Some(each), LPARAM(&mut wins as *mut _ as isize));
        for h in wins {
            let mut pid = 0;
            GetWindowThreadProcessId(h, Some(&mut pid));
            let proc = nova_sys::win::process_name(pid);
            if !["chrome", "msedge", "brave", "opera", "vivaldi", "firefox"].contains(&proc.as_str()) {
                continue;
            }
            let mut t = nova_sys::tabs::Tabs::new();
            let s = std::time::Instant::now();
            let first = t.json(h, &proc, false);
            let a = s.elapsed();
            let s = std::time::Instant::now();
            let again = t.json(h, &proc, false);
            let b = s.elapsed();
            println!("{proc}: first {:?} (search+read), next read {:?}\n  {}", a, b, &again[..again.len().min(300)]);
            let _ = first;
        }
    }
}
