// Lists the Tab / TabItem elements in each visible Chrome / Edge window (diagnostic, read only).
use windows::{core::BOOL, Win32::{Foundation::*, System::{Com::*, Variant::VARIANT}, UI::{Accessibility::*, WindowsAndMessaging::*}}};

unsafe extern "system" fn each(h: HWND, l: LPARAM) -> BOOL {
    let list = &mut *(l.0 as *mut Vec<HWND>);
    if IsWindowVisible(h).as_bool() && nova_sys::win::class_of(h) == "Chrome_WidgetWin_1" {
        list.push(h);
    }
    true.into()
}

fn main() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).unwrap();
        let mut wins: Vec<HWND> = Vec::new();
        let _ = EnumWindows(Some(each), LPARAM(&mut wins as *mut _ as isize));
        for h in wins {
            let mut pid = 0;
            GetWindowThreadProcessId(h, Some(&mut pid));
            let proc = nova_sys::win::process_name(pid);
            if proc != "chrome" && proc != "msedge" {
                continue;
            }
            let mut wr = RECT::default();
            let _ = GetWindowRect(h, &mut wr);
            println!("== {proc} window {:?} zoomed={}", (wr.left, wr.top, wr.right, wr.bottom), IsZoomed(h).as_bool());
            let root = uia.ElementFromHandle(h).unwrap();
            for (label, ct) in [("Tab", UIA_TabControlTypeId), ("TabItem", UIA_TabItemControlTypeId)] {
                let cond = uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(ct.0)).unwrap();
                let t = std::time::Instant::now();
                let all = root.FindAll(TreeScope_Descendants, &cond).unwrap();
                let n = all.Length().unwrap();
                println!("  {label}: {n} found in {:?}", t.elapsed());
                for i in 0..n.min(6) {
                    let e = all.GetElement(i).unwrap();
                    let r = e.CurrentBoundingRectangle().unwrap();
                    let parent = uia.ControlViewWalker().unwrap().GetParentElement(&e).ok();
                    let pt = parent.as_ref().and_then(|p| p.CurrentControlType().ok()).map(|c| c.0).unwrap_or(0);
                    println!("    [{i}] {:?} name={:?} parentType={pt}", (r.left, r.top, r.right - r.left, r.bottom - r.top), e.CurrentName().map(|s| s.to_string()).unwrap_or_default().chars().take(30).collect::<String>());
                }
            }
        }
    }
}
