//! Wall-clock time in an IANA zone ("Africa/Cairo"), from Windows' own time zone database: the one
//! the PC's clock uses, kept current by Windows Update. (The tz data inside Windows' ICU is frozen at
//! 2022g and misses Egypt's return to summer time in 2023; Chromium in 1.7 has 2025b.)
//! The IANA name is mapped to the Windows zone ("Egypt Standard Time") with ICU's own table.
use std::{cell::RefCell, collections::HashMap};

use windows::Win32::{
    Foundation::{FILETIME, SYSTEMTIME},
    Globalization::*,
    System::Time::*,
};

use crate::icu::Fields;

thread_local! {
    static ZONES: RefCell<HashMap<String, Option<DYNAMIC_TIME_ZONE_INFORMATION>>> = RefCell::new(HashMap::new());
}

fn windows_id(zone: &str) -> Option<String> {
    let z16: Vec<u16> = zone.encode_utf16().collect();
    let mut buf = [0u16; 128];
    let mut status = U_ZERO_ERROR;
    let n = unsafe { ucal_getWindowsTimeZoneID(z16.as_ptr(), z16.len() as i32, buf.as_mut_ptr(), buf.len() as i32, &mut status) };
    (n > 0 && status.0 <= U_ZERO_ERROR.0).then(|| String::from_utf16_lossy(&buf[..n as usize]))
}

fn dynamic_zone(zone: &str) -> Option<DYNAMIC_TIME_ZONE_INFORMATION> {
    ZONES.with(|cache| {
        if let Some(z) = cache.borrow().get(zone) {
            return *z;
        }
        // an IANA name through ICU's table, or already a Windows name (Outlook: "Egypt Standard Time")
        let found = Some(windows_id(zone).unwrap_or_else(|| zone.to_string())).and_then(|id| {
            let mut i = 0;
            loop {
                let mut info = DYNAMIC_TIME_ZONE_INFORMATION::default();
                if unsafe { EnumDynamicTimeZoneInformation(i, &mut info) } != 0 {
                    return None;
                }
                // the name ends at the first NUL (what follows it isn't part of it)
                let len = info.TimeZoneKeyName.iter().position(|&c| c == 0).unwrap_or(info.TimeZoneKeyName.len());
                if String::from_utf16_lossy(&info.TimeZoneKeyName[..len]) == id {
                    return Some(info);
                }
                i += 1;
            }
        });
        cache.borrow_mut().insert(zone.to_string(), found);
        found
    })
}

/// Local fields of a moment (ms since 1970) in `zone`; None when Windows doesn't know the zone.
pub fn local(zone: &str, ms: f64) -> Option<Fields> {
    let info = dynamic_zone(zone)?;
    // FILETIME counts 100 ns since 1601
    let ticks = (ms as i64 + 11_644_473_600_000) * 10_000;
    let ft = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    let mut utc = SYSTEMTIME::default();
    let mut loc = SYSTEMTIME::default();
    unsafe {
        windows::Win32::System::Time::FileTimeToSystemTime(&ft, &mut utc).ok()?;
        SystemTimeToTzSpecificLocalTimeEx(Some(&info), &utc, &mut loc).ok()?;
    }
    Some(Fields { year: loc.wYear as i32, month: loc.wMonth as u32, day: loc.wDay as u32, hour: loc.wHour as u32, minute: loc.wMinute as u32, second: loc.wSecond as u32 })
}
