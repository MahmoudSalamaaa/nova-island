//! Smart clipboard (smartclip.js): recognises what was just copied and offers the one useful thing.
//!   link  -> open it, or download it with IDM Nova when that is running
//!   color -> show the swatch
//!   math  -> the result (a click copies it)
//!   long text -> word count
//! Arithmetic goes through a tiny parser (numbers, + - * / ^ % and parentheses), never eval.
//! Lengths are in UTF-16 units and "whitespace" is JavaScript's, as in 1.7.
use nova_core::json::number;
use serde_json::{json, Value};

/// JavaScript's \s / String#trim set (WhiteSpace + LineTerminator, BOM included).
pub fn js_space(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

fn js_trim(s: &str) -> &str {
    s.trim_matches(js_space)
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

// --- arithmetic ---
struct Calc {
    s: Vec<char>,
    i: usize,
}

impl Calc {
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }
    fn num(&mut self) -> Result<f64, ()> {
        let start = self.i;
        let mut j = self.i;
        while self.s.get(j).is_some_and(|c| c.is_ascii_digit()) {
            j += 1;
        }
        if j == start {
            return Err(());
        }
        if self.s.get(j) == Some(&'.') && self.s.get(j + 1).is_some_and(|c| c.is_ascii_digit()) {
            j += 1;
            while self.s.get(j).is_some_and(|c| c.is_ascii_digit()) {
                j += 1;
            }
        }
        self.i = j;
        self.s[start..j].iter().collect::<String>().parse::<f64>().map_err(|_| ())
    }
    fn atom(&mut self) -> Result<f64, ()> {
        match self.peek() {
            Some('-') => {
                self.i += 1;
                Ok(-self.atom()?)
            }
            Some('+') => {
                self.i += 1;
                self.atom()
            }
            Some('(') => {
                self.i += 1;
                let v = self.expr()?;
                let close = self.peek();
                self.i += 1;
                if close != Some(')') {
                    return Err(());
                }
                Ok(v)
            }
            _ => {
                let mut v = self.num()?;
                if self.peek() == Some('%') {
                    self.i += 1;
                    v /= 100.0;
                }
                Ok(v)
            }
        }
    }
    fn power(&mut self) -> Result<f64, ()> {
        let b = self.atom()?;
        if self.peek() == Some('^') {
            self.i += 1;
            return Ok(js_pow(b, self.power()?));
        }
        Ok(b)
    }
    fn term(&mut self) -> Result<f64, ()> {
        let mut v = self.power()?;
        while matches!(self.peek(), Some('*' | '/')) {
            let op = self.s[self.i];
            self.i += 1;
            let r = self.power()?;
            v = if op == '*' { v * r } else { v / r };
        }
        Ok(v)
    }
    fn expr(&mut self) -> Result<f64, ()> {
        let mut v = self.term()?;
        while matches!(self.peek(), Some('+' | '-')) {
            let op = self.s[self.i];
            self.i += 1;
            let r = self.term()?;
            v = if op == '+' { v + r } else { v - r };
        }
        Ok(v)
    }
}

/// JavaScript's ** (V8 uses fdlibm's pow).
fn js_pow(b: f64, e: f64) -> f64 {
    crate::fdlibm::pow(b, e)
}

pub fn calc(src: &str) -> Option<f64> {
    let s: Vec<char> = src.chars().map(|c| match c { '×' => '*', '÷' => '/', c => c }).filter(|c| *c != ',' && !js_space(*c)).collect();
    let mut p = Calc { s, i: 0 };
    let v = p.expr().ok()?;
    (p.i == p.s.len() && v.is_finite()).then_some(v)
}

/// Number#toFixed(8): the exact value rounded half up (away from zero) at 8 decimals.
fn to_fixed8(v: f64) -> String {
    if v.abs() >= 1e21 {
        return number(v);
    }
    // the exact decimal expansion of a double ends within 1074 digits
    let exact = format!("{:.1100}", v.abs());
    let (int, frac) = exact.split_once('.').unwrap();
    let mut digits: Vec<u8> = format!("{int}{}", &frac[..8]).bytes().map(|b| b - b'0').collect();
    if frac.as_bytes()[8] >= b'5' {
        let mut k = digits.len();
        loop {
            if k == 0 {
                digits.insert(0, 1);
                break;
            }
            k -= 1;
            if digits[k] == 9 {
                digits[k] = 0;
            } else {
                digits[k] += 1;
                break;
            }
        }
    }
    let s: String = digits.iter().map(|d| (d + b'0') as char).collect();
    let (i, f) = s.split_at(s.len() - 8);
    format!("{}{}.{f}", if v < 0.0 { "-" } else { "" }, if i.is_empty() { "0" } else { i })
}

/// `Number.isInteger(v) ? String(v) : String(+v.toFixed(8))`
fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 {
        number(v)
    } else {
        number(to_fixed8(v).parse::<f64>().unwrap_or(f64::NAN))
    }
}

fn is_hex_color(t: &str) -> bool {
    let Some(h) = t.strip_prefix('#') else { return false };
    matches!(h.len(), 3 | 4 | 6 | 8) && h.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_fn_color(t: &str) -> bool {
    let lower = t.to_ascii_lowercase();
    for f in ["rgba(", "hsla(", "rgb(", "hsl("] {
        if let Some(rest) = lower.strip_prefix(f) {
            let Some(body) = rest.strip_suffix(')') else { return false };
            return !body.is_empty() && body.chars().all(|c| c.is_ascii_digit() || js_space(c) || ".,%/deg".contains(c));
        }
    }
    false
}

/// ^\d{1,4}([-/.])\d{1,4}\1\d{1,4}$  or  ^\+?\d+(-\d+)+$
fn is_date_or_phone(t: &str) -> bool {
    let run = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    for sep in ['-', '/', '.'] {
        let parts: Vec<&str> = t.split(sep).collect();
        if parts.len() == 3 && parts.iter().all(|p| run(p) && p.len() <= 4) {
            return true;
        }
    }
    let t = t.strip_prefix('+').unwrap_or(t);
    let parts: Vec<&str> = t.split('-').collect();
    parts.len() >= 2 && parts.iter().all(|p| run(p))
}

fn is_math_chars(t: &str) -> bool {
    t.chars().all(|c| c.is_ascii_digit() || js_space(c) || "+-*/().,%×÷^".contains(c))
}

/// /\d\s*[+\-*/×÷^%]\s*[\d(]/
fn has_operation(t: &str) -> bool {
    let c: Vec<char> = t.chars().collect();
    for i in 0..c.len() {
        if !c[i].is_ascii_digit() {
            continue;
        }
        let mut j = i + 1;
        while j < c.len() && js_space(c[j]) {
            j += 1;
        }
        if j >= c.len() || !"+-*/×÷^%".contains(c[j]) {
            continue;
        }
        let mut k = j + 1;
        while k < c.len() && js_space(c[k]) {
            k += 1;
        }
        if k < c.len() && (c[k].is_ascii_digit() || c[k] == '(') {
            return true;
        }
    }
    false
}

/// What a copy is: Ok(Some(..)) recognised, Ok(None) nothing, Err for a link the URL parser refuses
/// (1.7 throws there; the caller treats it as nothing).
pub fn classify(text: &str) -> Result<Option<Value>, String> {
    let t = js_trim(text);
    if t.is_empty() || utf16_len(t) > 4000 {
        return Ok(None);
    }
    let lower = t.to_ascii_lowercase();
    let after = lower.strip_prefix("https://").or(lower.strip_prefix("http://"));
    if after.is_some_and(|a| !a.is_empty() && !a.chars().any(js_space)) && utf16_len(t) <= 2000 {
        let host = crate::url::hostname(t)?;
        return Ok(Some(json!({ "type": "url", "url": t, "host": host.strip_prefix("www.").unwrap_or(&host) })));
    }
    if is_hex_color(t) || is_fn_color(t) {
        return Ok(Some(json!({ "type": "color", "color": t })));
    }
    // arithmetic: digits with at least one operator between them, but not dates or phone numbers
    if !is_date_or_phone(t) && utf16_len(t) <= 80 && is_math_chars(t) && has_operation(t) {
        if let Some(v) = calc(t) {
            return Ok(Some(json!({ "type": "math", "expr": t, "result": fmt_num(v) })));
        }
    }
    let words = t.split(js_space).filter(|w| !w.is_empty()).count();
    if words >= 30 {
        return Ok(Some(json!({ "type": "text", "words": words, "chars": utf16_len(t) })));
    }
    Ok(None)
}
