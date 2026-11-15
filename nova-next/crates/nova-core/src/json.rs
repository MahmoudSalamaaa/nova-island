//! JSON text exactly as 1.7 writes it: JavaScript's `JSON.stringify(value, null, 2)`.
//! Differences from serde_json's own pretty printer that matter for byte-identical files:
//!   numbers   JS Number#toString (5 not 5.0, 1e+21, 1.5e-7, -0 as 0)
//!   keys      integer-like keys ("2", "10") first in ascending order, then the rest as inserted
//!   escapes   only " \ and control characters (\b \f \n \r \t, else \u00xx); nothing else
use serde_json::{Map, Value};

/// `JSON.stringify(v, null, 2)`.
pub fn pretty(v: &Value) -> String {
    let mut out = String::new();
    write(&mut out, v, 0);
    out
}

fn write(out: &mut String, v: &Value, depth: usize) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&number(n.as_f64().unwrap_or(f64::NAN))),
        Value::String(s) => string(out, s),
        Value::Array(a) => {
            if a.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(out, depth + 1);
                write(out, x, depth + 1);
            }
            newline(out, depth);
            out.push(']');
        }
        Value::Object(o) => {
            if o.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, k) in js_key_order(o).into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(out, depth + 1);
                string(out, k);
                out.push_str(": ");
                write(out, &o[k], depth + 1);
            }
            newline(out, depth);
            out.push('}');
        }
    }
}

fn newline(out: &mut String, depth: usize) {
    out.push('\n');
    for _ in 0..depth {
        out.push_str("  ");
    }
}

/// JS property order: array-index keys ascending, then string keys in insertion order.
pub fn js_key_order(o: &Map<String, Value>) -> Vec<&String> {
    let index = |k: &str| -> Option<u32> {
        if k.is_empty() || (k.len() > 1 && k.starts_with('0')) || !k.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        k.parse::<u64>().ok().filter(|n| *n < u32::MAX as u64).map(|n| n as u32)
    };
    let mut ints: Vec<(&String, u32)> = o.keys().filter_map(|k| index(k).map(|n| (k, n))).collect();
    ints.sort_by_key(|(_, n)| *n);
    let mut keys: Vec<&String> = ints.into_iter().map(|(k, _)| k).collect();
    keys.extend(o.keys().filter(|k| index(k).is_none()));
    keys
}

fn string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// JavaScript's Number#toString (and JSON.stringify: NaN and Infinity become null).
pub fn number(x: f64) -> String {
    if !x.is_finite() {
        return "null".into();
    }
    if x == 0.0 {
        return "0".into();
    }
    let sign = if x < 0.0 { "-" } else { "" };
    // shortest round-trip digits: "d.ddde±x"
    let e = format!("{:e}", x.abs());
    let (mant, exp) = e.split_once('e').unwrap();
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exp.parse::<i32>().unwrap() + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let rest = if k > 1 { format!(".{}", &digits[1..]) } else { String::new() };
        format!("{}{rest}e{}{}", &digits[..1], if n - 1 >= 0 { "+" } else { "-" }, (n - 1).abs())
    };
    format!("{sign}{body}")
}

/// JavaScript's Math.round: halves go up (2.5 -> 3, -2.5 -> -2).
pub fn js_round(x: f64) -> f64 {
    let f = x.floor();
    if x - f >= 0.5 { f + 1.0 } else { f }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_like_js() {
        for (x, s) in [(5.0, "5"), (5.5, "5.5"), (1e21, "1e+21"), (1e-7, "1e-7"), (1.5e-7, "1.5e-7"), (1e20, "100000000000000000000"), (0.000001, "0.000001"), (-0.0, "0"), (0.1 + 0.2, "0.30000000000000004")] {
            assert_eq!(number(x), s);
        }
        assert_eq!(js_round(2.5), 3.0);
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!(js_round(2.4999), 2.0);
    }
}
