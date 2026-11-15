// Stands in for the Nova installer in update tests: installs nothing, only writes how it was
// started (its arguments) to fake-installer-ran.txt in the temp folder.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let _ = std::fs::write(std::env::temp_dir().join("fake-installer-ran.txt"), args.join(" "));
}
