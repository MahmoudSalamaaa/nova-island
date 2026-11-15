//! The calendar fields of a moment in an IANA time zone, from Windows' ICU (icuin.dll, Windows 10
//! 1703+). prayer.js gets the same answers from Chromium's ICU through Intl.DateTimeFormat.
use windows::{core::PCSTR, Win32::Globalization::*};

pub struct Fields {
    pub year: i32,
    pub month: u32, // 1-12
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

/// `zone` like "Africa/Cairo"; `calendar` "gregorian" or "islamic-umalqura". None when ICU can't
/// open the zone.
pub fn fields(zone: &str, calendar: &str, ms: f64) -> Option<Fields> {
    let zone16: Vec<u16> = zone.encode_utf16().collect();
    let locale = format!("en@calendar={calendar}\0");
    unsafe {
        let mut status = U_ZERO_ERROR;
        let cal = ucal_open(zone16.as_ptr(), zone16.len() as i32, PCSTR(locale.as_ptr()), UCAL_DEFAULT, &mut status);
        if cal.is_null() || status.0 > U_ZERO_ERROR.0 {
            return None;
        }
        ucal_setMillis(cal, ms, &mut status);
        let get = |f: UCalendarDateFields, status: &mut UErrorCode| ucal_get(cal as *const *const core::ffi::c_void, f, status);
        let out = Fields {
            year: get(UCAL_YEAR, &mut status),
            month: (get(UCAL_MONTH, &mut status) + 1) as u32,
            day: get(UCAL_DATE, &mut status) as u32,
            hour: get(UCAL_HOUR_OF_DAY, &mut status) as u32,
            minute: get(UCAL_MINUTE, &mut status) as u32,
            second: get(UCAL_SECOND, &mut status) as u32,
        };
        ucal_close(cal);
        (status.0 <= U_ZERO_ERROR.0).then_some(out)
    }
}

/// The PC's own time zone id (what Intl.DateTimeFormat().resolvedOptions().timeZone gives).
pub fn default_zone() -> String {
    let mut buf = [0u16; 128];
    unsafe {
        let mut status = U_ZERO_ERROR;
        let n = ucal_getDefaultTimeZone(buf.as_mut_ptr(), buf.len() as i32, &mut status);
        String::from_utf16_lossy(&buf[..n.clamp(0, 128) as usize])
    }
}
