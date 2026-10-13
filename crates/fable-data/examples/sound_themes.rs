//! Prints each map of a region with its sound themes and how many sound-map
//! blocks carry each value: `cargo run --example sound_themes -- <FABLE_DIR>/data StartOakVale`.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;

use fable_data::bbbb::BbbbFile;
use fable_data::lev::Lev;
use fable_data::wld::Wld;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(
        args.next()
            .ok_or("usage: sound_themes <data dir> <region>")?,
    );
    let region = args
        .next()
        .ok_or("usage: sound_themes <data dir> <region>")?;
    let wld = Wld::open(dir.join("Levels/FinalAlbion.wld"))?;
    let r = wld.region(&region).ok_or("no such region")?;
    let wad_path = dir.join("Levels/FinalAlbion.wad");
    let wad = BbbbFile::open(&wad_path)?;
    let mut file = BufReader::new(File::open(&wad_path)?);
    for map in &r.contains_maps {
        let Some(entry) = wad.index.by_name(&format!("Data\\Levels\\{map}")) else {
            println!("{map}: not in the wad");
            continue;
        };
        let data = entry.read_data(&mut file)?;
        let lev = Lev::parse(&data)?;
        let tail = lev.parse_tail(&data)?;
        let mut counts: BTreeMap<u8, usize> = BTreeMap::new();
        for b in &tail.sound_map {
            *counts.entry(b.value()).or_default() += 1;
        }
        println!(
            "{map}: value {:?}, themes {:?}",
            lev.sound_value, lev.sound_themes
        );
        println!("  block values {counts:?}");
        // Where each value's blocks are: their world bounding box and a sample.
        let m = wld.map(map).ok_or("map not in the wld")?;
        let columns = (lev.width / 4) as usize;
        for &value in counts.keys() {
            let cells: Vec<(f32, f32)> = tail
                .sound_map
                .iter()
                .enumerate()
                .filter(|(_, b)| b.value() == value)
                .map(|(i, _)| {
                    (
                        m.x as f32 + (i % columns) as f32 * 4.0 + 2.0,
                        m.y as f32 + (i / columns) as f32 * 4.0 + 2.0,
                    )
                })
                .collect();
            let (x0, x1) = cells
                .iter()
                .fold((f32::MAX, f32::MIN), |a, c| (a.0.min(c.0), a.1.max(c.0)));
            let (y0, y1) = cells
                .iter()
                .fold((f32::MAX, f32::MIN), |a, c| (a.0.min(c.1), a.1.max(c.1)));
            let theme = usize::from(value)
                .checked_sub(1)
                .and_then(|i| lev.sound_themes.get(i));
            println!(
                "  {value} {theme:?}: x {x0}..{x1}, y {y0}..{y1}, e.g. {:?}",
                cells[cells.len() / 2]
            );
        }
        let first: Vec<String> = tail
            .sound_map
            .iter()
            .take(4)
            .map(|b| format!("{:02x?}", b.raw))
            .collect();
        println!("  first blocks {}", first.join(" "));
    }
    Ok(())
}
