fn main() {
    let p = std::path::PathBuf::from(std::env::var("APPDATA").unwrap()).join("dynamic-nova/apps-cache.json");
    let v: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    let mut names: Vec<String> = v.iter().map(|a| a["name"].as_str().unwrap().to_string()).collect();
    names.reverse();
    names.sort_by(|a, b| nova_services::apps::locale_compare(a, b));
    println!("{}", serde_json::to_string(&names).unwrap());
}
