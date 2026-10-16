//! Prints a Lev map's grid point bytes 15 to 20 as pictures (one PGM per byte):
//! `cargo run --example lev_bytes -- <FABLE_DIR>/data StartOakValeWest.lev <out prefix>`.

use std::fs::File;
use std::io::BufReader;

use fable_data::bbbb::BbbbFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [dir, level, out] = a.as_slice() else {
        return Err("usage: lev_bytes <data dir> <level.lev> <out prefix>".into());
    };
    let wad_path = std::path::Path::new(dir).join("Levels/FinalAlbion.wad");
    let wad = BbbbFile::open(&wad_path)?;
    let mut file = BufReader::new(File::open(&wad_path)?);
    let entry = wad
        .index
        .by_name(&format!("Data\\Levels\\FinalAlbion\\{level}"))
        .ok_or("no such level")?;
    let lev = fable_data::lev::Lev::parse(&entry.read_data(&mut file)?)?;
    let (w, h) = (lev.width as usize + 1, lev.height as usize + 1);
    for byte in 15..21 {
        let values: Vec<u8> = lev.cells.iter().map(|c| c.raw[byte]).collect();
        let mut counts = std::collections::BTreeMap::new();
        for &v in &values {
            *counts.entry(v).or_insert(0) += 1;
        }
        let distinct = counts.len();
        let top: Vec<_> = counts.iter().take(6).collect();
        println!("byte {byte}: {distinct} distinct values, lowest {top:?}");
        let mut pgm = format!("P5 {w} {h} 255\n").into_bytes();
        // Flip so north is up.
        for row in (0..h).rev() {
            pgm.extend_from_slice(&values[row * w..(row + 1) * w]);
        }
        std::fs::write(format!("{out}_{byte}.pgm"), pgm)?;
    }
    Ok(())
}
