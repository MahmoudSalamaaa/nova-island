// nova-sign against signing.js (parity/oracle/gen-sign.cjs): the same canonical text, and the same
// yes/no for every real signed file, tampered copy and oddly written signature.
use std::{fs, path::PathBuf};

use serde_json::Value;

fn fixture() -> Value {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../parity/fixtures/sign.json");
    serde_json::from_str(&fs::read_to_string(p).expect("run parity/oracle/gen-sign.cjs first")).unwrap()
}

#[test]
fn canonical_like_signing_js() {
    for c in fixture()["canonical"].as_array().unwrap() {
        let v: Value = serde_json::from_str(c["value"].as_str().unwrap()).unwrap();
        assert_eq!(nova_sign::canonical(&v), c["canonical"].as_str().unwrap());
    }
}

#[test]
fn verify_like_signing_js() {
    for c in fixture()["verify"].as_array().unwrap() {
        let ok = nova_sign::verify(&c["body"], c["sig"].as_str());
        assert_eq!(ok, c["ok"].as_bool().unwrap(), "{}", c["what"]);
    }
}
