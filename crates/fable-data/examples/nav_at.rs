//! Prints a map's navigation sections (one per map state): their layers and
//! leaf counts, and the leaves under each given point (map coordinates).
//!
//! Usage: `cargo run --example nav_at -- <FABLE_DIR>/data <map> [x,y]...`

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};

use fable_data::bbbb::BbbbFile;
use fable_data::lev::Lev;
use fable_data::nav::Navigation;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().ok_or("usage")?);
    let map = args.next().ok_or("usage")?;
    let points: Vec<(f32, f32)> = args
        .map(|a| {
            let (x, y) = a.split_once(',').expect("x,y");
            (x.parse().expect("x"), y.parse().expect("y"))
        })
        .collect();
    let wad_path = dir.join("Levels/FinalAlbion.wad");
    let wad = BbbbFile::open(&wad_path)?;
    let entry = wad
        .index
        .by_name(&format!("Data\\Levels\\FinalAlbion\\{map}.lev"))
        .ok_or("no such map")?;
    let mut file = BufReader::new(File::open(&wad_path)?);
    file.seek(SeekFrom::Start(entry.offset as u64))?;
    let mut data = vec![0; entry.size as usize];
    file.read_exact(&mut data)?;
    let lev = Lev::parse(&data)?;
    let tail = lev.parse_tail(&data)?;
    for section in &tail.sections {
        let nav = Navigation::parse(&section.data, lev.width, lev.height)?;
        let mut per_layer = vec![0usize; nav.layers.len()];
        for (layer, _, _) in nav.leaves() {
            per_layer[layer] += 1;
        }
        println!("{}: layers {:?}", section.name, per_layer);
        for &(x, y) in &points {
            for (layer, node, leaf) in nav.leaves() {
                let h = node.size() / 2.0;
                let [cx, cy] = node.centre;
                if (x - cx).abs() <= h && (y - cy).abs() <= h {
                    println!(
                        "  ({x}, {y}): layer {layer} leaf {} size {} kind {} value {}",
                        node.id,
                        node.size(),
                        leaf.kind,
                        leaf.value
                    );
                }
            }
        }
    }
    Ok(())
}
