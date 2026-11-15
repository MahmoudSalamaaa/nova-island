//! Network companion (network.js): is there a link (Wi-Fi / cable), and does it actually reach the
//! internet. The link comes from the adapters' names only (reading the Wi-Fi name counts as a
//! location request on Windows 11, so it is never read). The internet check is Windows' own
//! connectivity probe, every 15 s (every 5 s while offline, to notice the return fast).
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use windows::Win32::{
    NetworkManagement::IpHelper::{GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST, IP_ADAPTER_ADDRESSES_LH},
    Networking::WinSock::{AF_INET, SOCKADDR_IN},
};

use crate::ctx::{now_ms, Ctx, Wake};

const PROBE: &str = "http://www.msftconnecttest.com/connecttest.txt";
const LINK_MS: u64 = 5000;

struct State {
    link: String,
    internet: Value, // null until the first probe, then true / false
    since: f64,
}

pub struct Network {
    ctx: Ctx,
    st: Mutex<State>,
    last_probe: Mutex<f64>,
    enabled: Mutex<bool>,
    wake: Wake,
    back: Mutex<Vec<Box<dyn Fn() + Send>>>,
}

/// "wifi" / "ethernet" / "none": any adapter with a real IPv4 address is up (Wi-Fi wins).
pub fn link_kind() -> &'static str {
    let mut kind = "none";
    let mut size = 32 * 1024u32;
    let mut buf = vec![0u8; size as usize];
    unsafe {
        let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
        if GetAdaptersAddresses(AF_INET.0 as u32, flags, None, Some(buf.as_mut_ptr() as *mut _), &mut size) != 0 {
            buf.resize(size as usize, 0);
            if GetAdaptersAddresses(AF_INET.0 as u32, flags, None, Some(buf.as_mut_ptr() as *mut _), &mut size) != 0 {
                return kind;
            }
        }
        let mut a = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
        while !a.is_null() {
            let ad = &*a;
            let mut up = false;
            let mut u = ad.FirstUnicastAddress;
            while !u.is_null() {
                let sa = (*u).Address.lpSockaddr;
                if !sa.is_null() && (*sa).sa_family == AF_INET {
                    let ip = (*(sa as *const SOCKADDR_IN)).sin_addr.S_un.S_un_b;
                    // not loopback (internal), not link-local 169.254.x
                    if ip.s_b1 != 127 && !(ip.s_b1 == 169 && ip.s_b2 == 254) {
                        up = true;
                    }
                }
                u = (*u).Next;
            }
            if up {
                let name = ad.FriendlyName.to_string().unwrap_or_default().to_lowercase();
                if name.contains("wifi") || name.contains("wi-fi") || name.contains("wlan") || name.contains("wireless") {
                    return "wifi";
                }
                kind = "ethernet";
            }
            a = ad.Next;
        }
    }
    kind
}

fn probe() -> bool {
    nova_core::http::get(PROBE, 4000).map(|r| r.ok() && r.text().starts_with("Microsoft Connect Test")).unwrap_or(false)
}

impl Network {
    pub fn new(ctx: Ctx) -> Arc<Network> {
        let on = ctx.on("netToasts");
        Arc::new(Network { ctx, st: Mutex::new(State { link: "none".into(), internet: Value::Null, since: now_ms() }), last_probe: Mutex::new(0.0), enabled: Mutex::new(on), wake: Wake::new(), back: Mutex::new(Vec::new()) })
    }

    pub fn start(self: &Arc<Self>) {
        let me = self.clone();
        let mut beat = 0u32;
        std::thread::spawn(move || loop {
            if *me.enabled.lock().unwrap() {
                me.poll();
                // (1.8.7) the ping for the System tab and the closed island: every 10 s, online only
                if me.st.lock().unwrap().internet == true {
                    if beat % 2 == 0 {
                        crate::netspeed::ping_now();
                    }
                    beat = beat.wrapping_add(1);
                } else {
                    crate::netspeed::ping_clear();
                }
                me.wake.sleep(LINK_MS);
            } else {
                me.wake.sleep(60 * 60 * 1000);
            }
        });
    }

    pub fn set_enabled(&self, on: bool) {
        let mut e = self.enabled.lock().unwrap();
        if *e != on {
            *e = on;
            drop(e);
            self.wake.wake();
        }
    }

    /// Called when the internet comes back.
    pub fn on_back(&self, f: impl Fn() + Send + 'static) {
        self.back.lock().unwrap().push(Box::new(f));
    }

    pub fn snapshot(&self) -> Value {
        let st = self.st.lock().unwrap();
        json!({ "link": st.link, "internet": st.internet, "since": st.since })
    }

    fn poll(&self) {
        let link = link_kind();
        let (old_link, old_internet) = {
            let st = self.st.lock().unwrap();
            (st.link.clone(), st.internet.clone())
        };
        let mut internet = old_internet.clone();
        let offline = internet == false || link == "none";
        if link == "none" {
            internet = json!(false);
        } else {
            let mut lp = self.last_probe.lock().unwrap();
            if now_ms() - *lp > if offline { 5000.0 } else { 15000.0 } || link != old_link {
                *lp = now_ms();
                drop(lp);
                internet = json!(probe());
            }
        }
        let changed = link != old_link || internet != old_internet;
        let prev = self.snapshot();
        {
            let mut st = self.st.lock().unwrap();
            st.link = link.to_string();
            st.internet = internet.clone();
            if changed {
                st.since = now_ms();
            }
        }
        if changed {
            let mut s = self.snapshot();
            s["prev"] = prev;
            self.ctx.send("network", s);
        }
        if changed && old_internet == false && internet == true {
            for f in self.back.lock().unwrap().iter() {
                f();
            }
        }
    }
}
