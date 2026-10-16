//! Writes one BIG archive entry's data to a file:
//! `cargo run --example big_raw -- <file.big> <bank> <entry name> <out>`.

use std::fs::File;
use std::io::BufReader;

use fable_data::big::BigFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [path, bank, name, out] = a.as_slice() else {
        return Err("usage: big_raw <file.big> <bank> <entry name> <out>".into());
    };
    let mut file = BufReader::new(File::open(path)?);
    let big = BigFile::read(&mut file)?;
    let index = big
        .bank(bank)
        .ok_or("no such bank")?
        .read_index(&mut file)?;
    let entry = index
        .entries
        .iter()
        .find(|e| &e.name == name)
        .ok_or("no such entry")?;
    std::fs::write(out, entry.read_data(&mut file)?)?;
    Ok(())
}
