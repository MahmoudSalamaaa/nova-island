//! Opening a file with its app (shell.openPath) or showing it in Explorer (shell.showItemInFolder).
use windows::{
    core::HSTRING,
    Win32::UI::{
        Shell::{SHOpenFolderAndSelectItems, SHParseDisplayName, ShellExecuteW},
        WindowsAndMessaging::SW_SHOWNORMAL,
    },
};

pub fn open_or_reveal(p: &str, reveal: bool) {
    unsafe {
        if !reveal {
            ShellExecuteW(None, &HSTRING::from("open"), &HSTRING::from(p), None, None, SW_SHOWNORMAL);
            return;
        }
        let mut pidl = std::ptr::null_mut();
        if SHParseDisplayName(&HSTRING::from(p), None, &mut pidl, 0, None).is_ok() && !pidl.is_null() {
            let _ = SHOpenFolderAndSelectItems(pidl, None, 0);
            windows::Win32::System::Com::CoTaskMemFree(Some(pidl as _));
        }
    }
}
