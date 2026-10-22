//! Writes every entry of a BIG bank into a directory, one file per entry:
//! `cargo run --example big_dump_all -- <file.big> <bank> <out dir>`.

use std::fs::File;
use std::io::BufReader;

use fable_data::big::BigFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [path, bank, out] = a.as_slice() else {
        return Err("usage: big_dump_all <file.big> <bank> <out dir>".into());
    };
    let mut file = BufReader::new(File::open(path)?);
    let big = BigFile::read(&mut file)?;
    let index = big
        .bank(bank)
        .ok_or("no such bank")?
        .read_index(&mut file)?;
    std::fs::create_dir_all(out)?;
    for e in &index.entries {
        let name: String = e
            .name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        std::fs::write(format!("{out}/{name}.bin"), e.read_data(&mut file)?)?;
    }
    println!("{} entries", index.entries.len());
    Ok(())
}
