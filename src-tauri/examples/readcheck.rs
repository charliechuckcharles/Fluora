// cargo run --release --example readcheck -- <tif> <k> <fx> <fy> <fw> <fh>: prints size and a few pixel values
#[path = "../src/tiffread.rs"]
mod tiffread;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let f: Vec<f64> = a[3..7].iter().map(|x| x.parse().unwrap()).collect();
    let t = std::time::Instant::now();
    let c = tiffread::read_window(std::path::Path::new(&a[1]), Some([f[0], f[1], f[2], f[3]]), a[2].parse().unwrap()).unwrap();
    println!("{}x{} ch={} {:?} um={:?} {:.2}s", c.w, c.h, c.data.len(), c.filters, c.um, t.elapsed().as_secs_f64());
    for ch in &c.data { println!("{:?}", [ch[0], ch[ch.len() / 2], ch[ch.len() - 1], ch[(c.w + 7) as usize]]); }
}
