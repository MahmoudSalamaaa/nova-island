// Prints where the Nova strip would sit on the taskbar (nova_sys::taskbar::find).
fn main() {
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        let _ = windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let uia: Option<windows::Win32::UI::Accessibility::IUIAutomation> = unsafe { windows::Win32::System::Com::CoCreateInstance(&windows::Win32::UI::Accessibility::CUIAutomation, None, windows::Win32::System::Com::CLSCTX_INPROC_SERVER).ok() };
    println!("{:?}", nova_sys::taskbar::find(uia.as_ref()));
}
