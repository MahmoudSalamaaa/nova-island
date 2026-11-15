//! The status line written exactly like the C# helper's Json class: one flat object, keys in the
//! order they are added, numbers rounded the .NET way (Math.Round: halves to even).

/// .NET Math.Round(v, digits): round half to even.
pub fn round(v: f64, digits: i32) -> f64 {
    let p = 10f64.powi(digits);
    let x = v * p;
    let r = if (x - x.trunc()).abs() == 0.5 { 2.0 * (x / 2.0).round() } else { x.round() };
    r / p
}

fn num(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    if v.fract() == 0.0 && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    format!("{v}")
}

/// A JSON string literal, escaped like the C# helper's.
pub fn quote(v: &str) -> String {
    let mut s = String::new();
    escape(&mut s, v);
    s
}

fn escape(out: &mut String, v: &str) {
    out.push('"');
    for c in v.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

pub struct Line {
    s: String,
    first: bool,
}

impl Line {
    pub fn new() -> Line {
        Line { s: "{".into(), first: true }
    }
    fn key(&mut self, k: &str) {
        if !self.first {
            self.s.push(',');
        }
        self.first = false;
        escape(&mut self.s, k);
        self.s.push(':');
    }
    pub fn str(mut self, k: &str, v: &str) -> Line {
        self.key(k);
        escape(&mut self.s, v);
        self
    }
    pub fn bool(mut self, k: &str, v: bool) -> Line {
        self.key(k);
        self.s.push_str(if v { "true" } else { "false" });
        self
    }
    pub fn int(mut self, k: &str, v: i64) -> Line {
        self.key(k);
        self.s.push_str(&v.to_string());
        self
    }
    /// C# Add(string, double): Math.Round(v, 3)
    pub fn num(mut self, k: &str, v: f64) -> Line {
        self.key(k);
        self.s.push_str(&num(round(v, 3)));
        self
    }
    pub fn raw(mut self, k: &str, json: &str) -> Line {
        self.key(k);
        self.s.push_str(json);
        self
    }
    pub fn done(mut self) -> String {
        self.s.push('}');
        self.s
    }
}

impl Default for Line {
    fn default() -> Self {
        Line::new()
    }
}
