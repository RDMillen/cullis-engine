//! Lists a BIG archive's banks and the first entries of each:
//! `cargo run --example big_list -- <file.big> [entries per bank]`.

use std::fs::File;
use std::io::BufReader;

use fable_data::big::BigFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("usage: big_list <file.big> [n]")?;
    let n: usize = args.next().map_or(Ok(10), |s| s.parse())?;
    let mut file = BufReader::new(File::open(&path)?);
    let big = BigFile::read(&mut file)?;
    for bank in &big.banks {
        println!("bank {} ({} entries)", bank.name, bank.entry_count);
        let index = bank.read_index(&mut file)?;
        for t in &index.type_counts {
            println!("  type {} x {}", t.entry_type, t.count);
        }
        for e in index.entries.iter().take(n) {
            println!(
                "  {} type {} size {} name {:?} sources {:?} info {} bytes {:02x?}",
                e.id,
                e.entry_type,
                e.size,
                e.name,
                e.sources,
                e.info.len(),
                &e.info[..e.info.len().min(32)]
            );
        }
    }
    Ok(())
}
