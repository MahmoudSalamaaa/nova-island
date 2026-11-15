// The smart clipboard against the current smartclip.js (parity/oracle/gen-smartclip.cjs).
use std::{fs, path::PathBuf};

use serde_json::Value;

/// A maths result that differs only in the last bit of a double. V8's pow (an fdlibm variant) is
/// sometimes one bit off for huge results such as 72^157, where 1.8's answer is the correctly
/// rounded one; the text differs in the 16th digit only.
fn last_bit_only(a: &Value, b: &Value) -> bool {
    if a["type"] != "math" || b["type"] != "math" || a["expr"] != b["expr"] {
        return false;
    }
    let (Some(x), Some(y)) = (a["result"].as_str().and_then(|s| s.parse::<f64>().ok()), b["result"].as_str().and_then(|s| s.parse::<f64>().ok())) else { return false };
    x.is_finite() && y.is_finite() && (x.to_bits() as i64 - y.to_bits() as i64).abs() <= 2
}

#[test]
fn smartclip_like_smartclip_js() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../parity/fixtures/smartclip.json");
    let cases: Value = serde_json::from_str(&fs::read_to_string(p).expect("run parity/oracle/gen-smartclip.cjs first")).unwrap();
    let mut wrong = Vec::new();
    for c in cases.as_array().unwrap() {
        let text = c["text"].as_str().unwrap();
        let got = nova_services::smartclip::classify(text);
        let ok = match (&got, c.get("throws")) {
            (Err(e), Some(t)) => e == t.as_str().unwrap(),
            (Ok(v), None) => {
                let v = v.clone().unwrap_or(Value::Null);
                v == c["out"] || last_bit_only(&v, &c["out"])
            }
            _ => false,
        };
        if !ok {
            wrong.push(format!("{text:?}: got {got:?} want {}", c.get("throws").unwrap_or(&c["out"])));
        }
    }
    assert!(wrong.is_empty(), "{} of {} differ:\n{}", wrong.len(), cases.as_array().unwrap().len(), wrong.iter().take(20).cloned().collect::<Vec<_>>().join("\n"));
}
