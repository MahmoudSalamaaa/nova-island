// The screenshot toast's thumbnail is made on a fresh thread (nova-app main.rs): it must work there.
#[test]
fn bgra_png_on_a_fresh_thread() {
    let (w, h) = (320u32, 180u32);
    let px: Vec<u8> = (0..w * h).flat_map(|i| [(i % 255) as u8, 80, 160, 255]).collect();
    let png = std::thread::spawn(move || nova_sys::icons::bgra_png(w, h, &px, Some(72))).join().unwrap();
    let png = png.expect("no thumbnail");
    assert_eq!(&png[1..4], b"PNG");
}

// A picture on the Shelf shows its own thumbnail (NOVA_THUMB_FILE: a picture to try).
#[test]
fn file_thumb_of_a_picture() {
    let Ok(path) = std::env::var("NOVA_THUMB_FILE") else { return };
    let icon = nova_sys::icons::file_icon(&path);
    let thumb = std::thread::spawn(move || nova_sys::icons::file_thumb(&path, 88)).join().unwrap();
    println!("icon {} chars, thumb {} chars", icon.len(), thumb.len());
    assert!(thumb.len() > icon.len() + 200, "no thumbnail, only the icon");
}
