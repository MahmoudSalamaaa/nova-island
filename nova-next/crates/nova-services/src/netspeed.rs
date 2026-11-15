//! (1.8.7) Internet use.
//! - Live ↓ / ↑: the network cards' own byte counters, read with the CPU / RAM stats. Uses no data.
//! - Ping: the time to open a connection to Cloudflare's 1.1.1.1:443, every 10 s while online
//!   (network.rs's loop). Under 1 KB a minute.
use std::{
    net::{SocketAddr, TcpStream},
    sync::atomic::{AtomicI64, Ordering},
    time::{Duration, Instant},
};

use windows::Win32::NetworkManagement::{
    IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2},
    Ndis::IfOperStatusUp,
};

/// The last ping in ms; -1 when there is none (offline, or not measured yet).
pub static PING_MS: AtomicI64 = AtomicI64::new(-1);

const PING_TO: &str = "1.1.1.1:443";
const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;

/// Bytes received and sent so far by the PC's real network cards (not loopback, not the filter
/// layers that would count the same bytes twice, not adapters with nothing plugged in).
pub fn counters() -> (u64, u64) {
    let (mut rx, mut tx) = (0u64, 0u64);
    unsafe {
        let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        if GetIfTable2(&mut table).is_err() || table.is_null() {
            return (0, 0);
        }
        let n = (*table).NumEntries as usize;
        let rows = std::slice::from_raw_parts((*table).Table.as_ptr(), n);
        for r in rows {
            let f = r.InterfaceAndOperStatusFlags._bitfield;
            let (hardware, filter, connector) = (f & 1 != 0, f & 2 != 0, f & 4 != 0);
            if r.Type == IF_TYPE_SOFTWARE_LOOPBACK || r.OperStatus != IfOperStatusUp || !hardware || filter || !connector {
                continue;
            }
            rx += r.InOctets;
            tx += r.OutOctets;
        }
        FreeMibTable(table as *const _);
    }
    (rx, tx)
}

/// The time to open a TCP connection, in ms.
fn connect_ms() -> Option<i64> {
    let addr: SocketAddr = PING_TO.parse().ok()?;
    let t = Instant::now();
    TcpStream::connect_timeout(&addr, Duration::from_millis(2000)).ok()?;
    Some(t.elapsed().as_millis().max(1) as i64)
}

/// Measures the ping now (network.rs calls it every 10 s while online).
pub fn ping_now() {
    PING_MS.store(connect_ms().unwrap_or(-1), Ordering::Relaxed);
}

pub fn ping_clear() {
    PING_MS.store(-1, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    #[test]
    fn counters_grow() {
        let (rx, tx) = super::counters();
        // a PC running tests has had some traffic; mostly this checks the call doesn't fail
        assert!(rx > 0 || tx > 0);
    }
}
