// nova-core against the references produced by the current JS (parity/oracle/gen-core.cjs).
use std::{fs, path::PathBuf};

use nova_core::{features::Features, json, settings::Store};
use serde_json::Value;

fn fixture(name: &str) -> Value {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../parity/fixtures").join(name);
    serde_json::from_str(&fs::read_to_string(p).expect("run parity/oracle/gen-core.cjs first")).unwrap()
}

/// Every JSON text settings.js wrote comes out of json::pretty byte for byte.
#[test]
fn json_text_like_settings_js() {
    for case in fixture("json.json").as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        let v: Value = serde_json::from_str(text).unwrap();
        assert_eq!(json::pretty(&v), text, "for {text:?}");
    }
}

/// 4000 random patches through features.set: the same file text as features.js after every one.
/// Ignored: the Rust app's defaults differ from 1.7's on purpose (Madar, the thin line, ...),
/// and the fixture is 1.7's. Run with `cargo test -- --ignored` to see the differences.
#[test]
#[ignore = "the defaults differ from 1.7 on purpose"]
fn features_like_features_js() {
    let f = fixture("features.json");
    let dir = std::env::temp_dir().join(format!("nova-core-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let mut features = Features::load(Store::new(dir.clone()));
    assert_eq!(features.get(), f["defaults"], "defaults");
    let read = || fs::read_to_string(dir.join("features.json")).unwrap_or_default();
    for (i, step) in f["steps"].as_array().unwrap().iter().enumerate() {
        let patch: Value = serde_json::from_str(step["patch"].as_str().unwrap()).unwrap();
        features.set(&patch);
        assert_eq!(read(), step["file"].as_str().unwrap(), "step {i}: patch {patch}");
    }
    features.reset();
    assert_eq!(read(), f["resetText"].as_str().unwrap(), "reset");
    let _ = fs::remove_dir_all(&dir);
}
