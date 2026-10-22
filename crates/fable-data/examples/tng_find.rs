//! Counts, per level, the things whose definition contains a word:
//! `cargo run --example tng_find -- <FABLE_DIR>/data STREETLAMP_LIT`.

use std::fs::File;
use std::io::BufReader;

use fable_data::bbbb::BbbbFile;
use fable_data::tng::Tng;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [dir, word] = a.as_slice() else {
        return Err("usage: tng_find <data dir> <word>".into());
    };
    let wad_path = std::path::Path::new(dir).join("Levels/FinalAlbion.wad");
    let wad = BbbbFile::open(&wad_path)?;
    let mut file = BufReader::new(File::open(&wad_path)?);
    for entry in wad
        .index
        .entries
        .iter()
        .filter(|e| e.name.ends_with(".tng"))
    {
        let text = String::from_utf8_lossy(&entry.read_data(&mut file)?).into_owned();
        let Ok(tng) = Tng::parse(&text) else { continue };
        let n = tng
            .things
            .iter()
            .filter(|t| t.definition().is_some_and(|d| d.contains(word.as_str())))
            .count();
        if n > 0 {
            println!("{n:4} {}", entry.name);
        }
    }
    Ok(())
}
