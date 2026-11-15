//! AmerTech's public key (signing.js): news items and plugins from the Nova team are signed with its
//! private key (kept off GitHub), so a changed or foreign file is never shown or installed.
use ed25519_dalek::{Signature, VerifyingKey};
use serde_json::Value;

/// The raw 32-byte Ed25519 key inside signing.js's SPKI
/// MCowBQYDK2VwAyEAB9MjI9rEIq86S4Wx0diaLPKJY+jX8rQhhu5jQqe//hk= (12-byte SPKI header + key).
const PUBLIC_KEY_SPKI_B64: &str = "MCowBQYDK2VwAyEAB9MjI9rEIq86S4Wx0diaLPKJY+jX8rQhhu5jQqe//hk=";

fn public_key() -> VerifyingKey {
    let spki = base64_node(PUBLIC_KEY_SPKI_B64);
    let key: [u8; 32] = spki[12..44].try_into().expect("Ed25519 SPKI is 44 bytes");
    VerifyingKey::from_bytes(&key).expect("valid key")
}

/// One fixed text for an object: keys sorted at every level (JavaScript's default sort, by UTF-16
/// code units), so the signature doesn't depend on how the file happens to be formatted.
pub fn canonical(v: &Value) -> String {
    match v {
        Value::Array(a) => format!("[{}]", a.iter().map(canonical).collect::<Vec<_>>().join(",")),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            let parts: Vec<String> = keys.iter().map(|k| format!("{}:{}", scalar(&Value::String((*k).clone())), canonical(&o[*k]))).collect();
            format!("{{{}}}", parts.join(","))
        }
        other => scalar(other),
    }
}

/// JSON.stringify of a single value (no spaces): the same text nova-core writes, minus indentation.
fn scalar(v: &Value) -> String {
    nova_core::json::pretty(v)
}

/// Buffer.from(s, 'base64'): lenient like Node. URL-safe letters count, anything else is skipped,
/// and decoding stops at the first '='.
pub fn base64_node(s: &str) -> Vec<u8> {
    let mut bits = 0u32;
    let mut n = 0;
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            _ => continue,
        };
        bits = (bits << 6) | v as u32;
        n += 6;
        if n >= 8 {
            n -= 8;
            out.push((bits >> n) as u8);
        }
    }
    out
}

/// True when `sig` (base64) is AmerTech's signature of `obj`.
pub fn verify(obj: &Value, sig: Option<&str>) -> bool {
    let Some(sig) = sig else { return false };
    let Ok(bytes) = <[u8; 64]>::try_from(base64_node(sig).as_slice()) else { return false };
    let signature = Signature::from_bytes(&bytes);
    public_key().verify_strict(canonical(obj).as_bytes(), &signature).is_ok()
}
