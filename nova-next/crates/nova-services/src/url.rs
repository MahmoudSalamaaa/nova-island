//! `new URL(s).hostname` for http(s) links (smartclip.js), following the WHATWG URL standard's host
//! parsing: user info and port dropped, the port checked, IPv4 written out in full, IPv6 kept in
//! brackets, forbidden characters refused, and non-ASCII domains turned into punycode (xn--).
//! Err("Invalid URL") where the browser's URL parser throws.

const INVALID: &str = "Invalid URL";

pub fn hostname(url: &str) -> Result<String, String> {
    let rest = url.split_once("://").map(|(_, r)| r).ok_or(INVALID)?;
    // special schemes: \ counts as /, and leading slashes are skipped
    let rest = rest.trim_start_matches(['/', '\\']);
    let end = rest.find(['/', '\\', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let host_port = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
    let (host, port) = if host_port.starts_with('[') {
        let close = host_port.find(']').ok_or(INVALID)?;
        let after = &host_port[close + 1..];
        let port = match after.strip_prefix(':') {
            Some(p) => Some(p),
            None if after.is_empty() => None,
            None => return Err(INVALID.into()),
        };
        (&host_port[..=close], port)
    } else {
        match host_port.split_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (host_port, None),
        }
    };
    if let Some(p) = port {
        if !p.bytes().all(|b| b.is_ascii_digit()) || (!p.is_empty() && p.parse::<u64>().map_or(true, |n| n > 65535)) {
            return Err(INVALID.into());
        }
    }
    if host.is_empty() {
        return Err(INVALID.into());
    }
    if let Some(v6) = host.strip_prefix('[') {
        let inner = v6.strip_suffix(']').ok_or(INVALID)?;
        return ipv6(inner).map(|s| format!("[{s}]"));
    }
    let decoded = percent_decode(host)?;
    let ascii = domain_to_ascii(&decoded)?;
    if ends_in_number(&ascii) {
        return ipv4(&ascii);
    }
    Ok(ascii)
}

fn percent_decode(s: &str) -> Result<String, String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (b.get(i + 1).and_then(|c| (*c as char).to_digit(16)), b.get(i + 2).and_then(|c| (*c as char).to_digit(16))) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).map_err(|_| INVALID.into())
}

/// Lower-cases, refuses forbidden code points, and punycodes each non-ASCII label.
fn domain_to_ascii(d: &str) -> Result<String, String> {
    let lower = d.to_lowercase();
    let mut labels = Vec::new();
    for label in lower.split(['.', '\u{3002}', '\u{ff0e}', '\u{ff61}']) {
        if label.is_ascii() {
            labels.push(label.to_string());
        } else {
            labels.push(format!("xn--{}", punycode(label)));
        }
    }
    let out = labels.join(".");
    let forbidden = |c: char| c <= '\u{1f}' || c == '\u{7f}' || " #%/:<>?@[\\]^|".contains(c);
    if out.is_empty() || out.chars().any(forbidden) {
        return Err(INVALID.into());
    }
    Ok(out)
}

/// RFC 3492 punycode of one label.
fn punycode(label: &str) -> String {
    const BASE: u32 = 36;
    let (tmin, tmax, skew, damp) = (1u32, 26u32, 38u32, 700u32);
    let input: Vec<u32> = label.chars().map(|c| c as u32).collect();
    let mut out: String = input.iter().filter(|c| **c < 0x80).map(|c| char::from_u32(*c).unwrap()).collect();
    let basic = out.len() as u32;
    let mut h = basic;
    if basic > 0 {
        out.push('-');
    }
    let digit = |d: u32| (if d < 26 { b'a' + d as u8 } else { b'0' + (d - 26) as u8 }) as char;
    let adapt = |mut delta: u32, numpoints: u32, first: bool| {
        delta = if first { delta / damp } else { delta / 2 };
        delta += delta / numpoints;
        let mut k = 0;
        while delta > ((BASE - tmin) * tmax) / 2 {
            delta /= BASE - tmin;
            k += BASE;
        }
        k + (BASE - tmin + 1) * delta / (delta + skew)
    };
    let (mut n, mut delta, mut bias) = (0x80u32, 0u32, 72u32);
    while (h as usize) < input.len() {
        let m = *input.iter().filter(|c| **c >= n).min().unwrap();
        delta += (m - n) * (h + 1);
        n = m;
        for &c in &input {
            if c < n {
                delta += 1;
            }
            if c == n {
                let mut q = delta;
                let mut k = BASE;
                loop {
                    let t = if k <= bias { tmin } else if k >= bias + tmax { tmax } else { k - bias };
                    if q < t {
                        break;
                    }
                    out.push(digit(t + (q - t) % (BASE - t)));
                    q = (q - t) / (BASE - t);
                    k += BASE;
                }
                out.push(digit(q));
                bias = adapt(delta, h + 1, h == basic);
                delta = 0;
                h += 1;
            }
        }
        delta += 1;
        n += 1;
    }
    out
}

/// The WHATWG "ends in a number" check: the last non-empty label is all digits or 0x-hex.
fn ends_in_number(d: &str) -> bool {
    let mut labels: Vec<&str> = d.split('.').collect();
    if labels.last() == Some(&"") && labels.len() > 1 {
        labels.pop();
    }
    let last = labels.last().copied().unwrap_or("");
    !last.is_empty() && (last.bytes().all(|b| b.is_ascii_digit()) || parse_ipv4_number(last).is_some())
}

fn parse_ipv4_number(s: &str) -> Option<u64> {
    if s.is_empty() {
        return None;
    }
    let (digits, radix) = if let Some(h) = s.strip_prefix("0x").or(s.strip_prefix("0X")) {
        (h, 16)
    } else if s.len() > 1 && s.starts_with('0') {
        (&s[1..], 8)
    } else {
        (s, 10)
    };
    if digits.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(digits, radix).ok()
}

/// IPv4 written out in full: 0x7f.1 -> 127.0.0.1.
fn ipv4(d: &str) -> Result<String, String> {
    let mut parts: Vec<&str> = d.split('.').collect();
    if parts.last() == Some(&"") && parts.len() > 1 {
        parts.pop();
    }
    if parts.len() > 4 {
        return Err(INVALID.into());
    }
    let nums: Vec<u64> = parts.iter().map(|p| parse_ipv4_number(p).ok_or(INVALID)).collect::<Result<_, _>>()?;
    let (last, head) = nums.split_last().unwrap();
    if head.iter().any(|n| *n > 255) || *last >= 256u64.pow(5 - nums.len() as u32) {
        return Err(INVALID.into());
    }
    let mut v = *last;
    for (i, n) in head.iter().enumerate() {
        v += n * 256u64.pow(3 - i as u32);
    }
    Ok(format!("{}.{}.{}.{}", v >> 24, (v >> 16) & 255, (v >> 8) & 255, v & 255))
}

/// IPv6, compressed the way the browser writes it (longest run of zero pieces as ::).
fn ipv6(s: &str) -> Result<String, String> {
    let addr: std::net::Ipv6Addr = s.parse().map_err(|_| INVALID)?;
    let seg = addr.segments();
    // the longest run (2+) of zeros, first one on a tie
    let (mut best, mut best_len, mut i) = (None, 1, 0);
    while i < 8 {
        if seg[i] == 0 {
            let start = i;
            while i < 8 && seg[i] == 0 {
                i += 1;
            }
            if i - start > best_len {
                best = Some(start);
                best_len = i - start;
            }
        } else {
            i += 1;
        }
    }
    let mut out = String::new();
    let mut i = 0;
    while i < 8 {
        if Some(i) == best {
            out.push_str(if i == 0 { "::" } else { ":" });
            i += best_len;
            continue;
        }
        out.push_str(&format!("{:x}", seg[i]));
        if i < 7 {
            out.push(':');
        }
        i += 1;
    }
    Ok(out)
}
