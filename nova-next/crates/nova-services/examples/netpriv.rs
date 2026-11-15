fn main() {
    println!("{}", serde_json::json!({ "link": nova_services::network::link_kind(), "mic": nova_services::privacy::query("microphone"), "cam": nova_services::privacy::query("webcam") }));
}
