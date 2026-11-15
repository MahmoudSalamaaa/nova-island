//! The widgets as PNG pictures (promo posters): drawn by the desktop's own code.
//!   cargo run --release -p nova-render --example shots -- <data folder> <out folder> [scale] [names…]
use std::path::PathBuf;

use nova_render::{desktop, font::Fonts, preview, state::Store};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let dir = PathBuf::from(&a[0]);
    let out = PathBuf::from(&a[1]);
    let scale: f32 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let names: Vec<&str> = if a.len() > 3 { a[3..].iter().map(String::as_str).collect() } else { desktop::WIDGETS.iter().map(|w| w.0).collect() };
    std::fs::create_dir_all(&out).unwrap();
    let fonts = Fonts::new().unwrap();
    let store = Store::open(Some(dir.join("widgets-state.json")));
    for n in names {
        let mut f = desktop::face(n, &dir, &store).expect(n);
        let png = preview::png(f.as_mut(), scale, &fonts).expect(n);
        std::fs::write(out.join(format!("{n}.png")), png).unwrap();
        println!("{n}.png");
    }
}
