// Quick check of nova_core::http against the services Nova reads.
fn main() {
    for url in ["https://ipwho.is/", "https://api.open-meteo.com/v1/forecast?latitude=30.04&longitude=31.24&current=temperature_2m,weather_code,is_day&daily=temperature_2m_max,temperature_2m_min&timezone=auto&forecast_days=1", "http://www.msftconnecttest.com/connecttest.txt", "https://raw.githubusercontent.com/NOVA-TECH-KIT/nova-island/main/news.json"] {
        match nova_core::http::get(url, 10000) {
            Ok(r) => println!("{} {} bytes  {}", r.status, r.body.len(), url.split('?').next().unwrap()),
            Err(e) => println!("ERR {e}  {url}"),
        }
    }
}
