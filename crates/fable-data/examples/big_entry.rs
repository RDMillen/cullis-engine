//! Prints one BIG archive entry's index fields and the start of its data:
//! `cargo run --example big_entry -- <file.big> <bank> <entry name or id>`.

use std::fs::File;
use std::io::BufReader;

use fable_data::big::BigFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let usage = "usage: big_entry <file.big> <bank> <entry name or id>";
    let (path, bank, key) = (
        args.next().ok_or(usage)?,
        args.next().ok_or(usage)?,
        args.next().ok_or(usage)?,
    );
    let mut file = BufReader::new(File::open(&path)?);
    let big = BigFile::read(&mut file)?;
    let bank = big.bank(&bank).ok_or("no such bank")?;
    let index = bank.read_index(&mut file)?;
    let entry = index
        .entries
        .iter()
        .find(|e| e.name == key || e.id.to_string() == key)
        .ok_or("no such entry")?;
    println!(
        "{} {:?} type {} size {} unknown_a {} unknown_b {} sources {:?}",
        entry.id,
        entry.name,
        entry.entry_type,
        entry.size,
        entry.unknown_a,
        entry.unknown_b,
        entry.sources
    );
    println!("info {:02x?}", entry.info);
    let data = entry.read_data(&mut file)?;
    for (i, row) in data.chunks(16).take(64).enumerate() {
        let ascii: String = row
            .iter()
            .map(|&b| {
                if (32..127).contains(&b) {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();
        println!("{:04x}  {:02x?}  {ascii}", i * 16, row);
    }
    Ok(())
}
