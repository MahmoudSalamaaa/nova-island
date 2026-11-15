//! CPU and memory (system.js): CPU from the change in Windows' idle / kernel / user times (what
//! Node's os.cpus() sums), memory from GlobalMemoryStatusEx (os.totalmem / os.freemem).
use windows::Win32::{
    Foundation::FILETIME,
    System::{SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX}, Threading::GetSystemTimes},
};

fn ticks(f: FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

/// (idle, total) since boot; kernel time includes idle.
pub fn cpu_times() -> (u64, u64) {
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe {
        let _ = GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user));
    }
    (ticks(idle), ticks(kernel) + ticks(user))
}

/// (total bytes, free bytes)
pub fn memory() -> (f64, f64) {
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    unsafe {
        let _ = GlobalMemoryStatusEx(&mut m);
    }
    (m.ullTotalPhys as f64, m.ullAvailPhys as f64)
}
