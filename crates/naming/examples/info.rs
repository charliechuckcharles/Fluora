fn main() { println!("{:?}", fluora_naming::tiffinfo::read(std::path::Path::new(&std::env::args().nth(1).unwrap()))); }
