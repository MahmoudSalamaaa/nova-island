fn main() {
    let r = nova_core::http::get("https://raw.githubusercontent.com/NOVA-TECH-KIT/nova-island/main/news.json", 15000).unwrap();
    let list: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
    for i in list.as_array().unwrap() { println!("{} valid={}", i["id"], nova_services::news::valid(i)); }
}
