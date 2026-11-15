fn main() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../parity/fixtures/plugins.json");
    let f: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    for (k, c) in f.as_object().unwrap() {
        let got = nova_services::plugins::check_bundle(&c["bundle"]).unwrap_or(serde_json::Value::Null);
        println!("{k}: {}", if got == c["manifest"] { "SAME" } else { "DIFFERENT" });
    }
}
