// Step 3's mini-player link check, the safe path only: a title no tab has, so neither side selects
// any tab; both just read the address bars. Also a browser that isn't running.
fn main() {
    let helper = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../helper/nova-helper.exe");
    for (proc, title) in [("chrome", "zz no such video title 9431"), ("msedge", "zz no such video title 9431"), ("nosuchbrowser", "x")] {
        let out = std::process::Command::new(&helper).args(["yturl", proc, title]).output().unwrap();
        let cs = String::from_utf8_lossy(&out.stdout).trim().trim_start_matches('\u{feff}').to_string();
        let rs = nova_sys::yturl::yt_url(proc, title);
        println!("{proc:14} C# {cs}\n{:14} Rust {rs}  -> {}", "", if cs == rs { "IDENTICAL" } else { "DIFFER" });
    }
}
