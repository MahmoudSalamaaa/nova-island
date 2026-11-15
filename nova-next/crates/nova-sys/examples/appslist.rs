// Step 3's apps-list check: `nova-helper apps` (C#) and nova_sys::apps::scan() must give the same
// list (same order, names, ids, icons and kinds).
use serde_json::Value;

fn main() {
    let helper = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../helper/nova-helper.exe");
    let out = std::process::Command::new(helper).arg("apps").output().unwrap();
    let cs: Vec<Value> = serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim_start_matches('\u{feff}').trim()).unwrap();
    let rs: Vec<Value> = serde_json::from_str(&nova_sys::apps::scan()).unwrap();
    println!("C# {} apps, Rust {} apps", cs.len(), rs.len());
    let mut diffs = 0;
    for i in 0..cs.len().max(rs.len()) {
        let (a, b) = (cs.get(i), rs.get(i));
        if a != b {
            diffs += 1;
            if diffs <= 8 {
                println!("#{i}\n  C#   {}\n  Rust {}", a.map(Value::to_string).unwrap_or_default(), b.map(Value::to_string).unwrap_or_default());
            }
        }
    }
    println!("{} of {} entries differ", diffs, cs.len().max(rs.len()));
}
