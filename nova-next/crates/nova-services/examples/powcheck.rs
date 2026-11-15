fn main() {
    for (b, e) in [(72.0, 157.0), (432.0, 55.0), (449.0, 90.284), (54.0, 48.0562), (1.1, 300.0), (8.0, 89.21)] {
        let f: f64 = nova_services::fdlibm::pow(b, e);
        let c: f64 = f64::powf(b, e);
        println!("{b}^{e}  fdlibm {f:e}  crt {c:e}");
    }
}
