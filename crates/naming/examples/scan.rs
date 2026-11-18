// cargo run --release --example scan -- <folder>   (read-only: prints what detection finds)
fn main() {
    let root = std::env::args().nth(1).expect("folder");
    let t = std::time::Instant::now();
    let det = fluora_naming::detect(std::path::Path::new(&root), "CNH", &|_| {});
    let images: usize = det.groups.iter().map(|g| g.entries.iter().filter(|e| e.kind == fluora_naming::Kind::Image).count()).sum();
    println!("{} groups, {} images, {} packed CZIs, {:.1}s", det.groups.len(), images, det.packed.len(), t.elapsed().as_secs_f64());
    for g in det.groups.iter().take(3) {
        println!("{} shared={:?} guessed={:?} filters={:?} um={:?}", g.rel, g.shared, g.guessed, g.filters, g.um);
        for e in g.entries.iter().take(2) {
            println!("   {:?} named={} split={} parts={} im={} first={}", e.kind, e.named, e.split, e.parts.len(), e.fields.im, e.parts[0].file.name);
        }
    }
    let p = fluora_naming::rename::plan(&det);
    println!("renames needed: {}, problems: {}", p.renames.len(), p.problems.len());
    for x in p.problems.iter().take(5) { println!("  {:?}", x); }
    for r in p.renames.iter().take(3) { println!("  {} -> {}", r.from, r.to); }
}
