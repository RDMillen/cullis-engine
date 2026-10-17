//! Prints the things in a level's Tng that have a component, with its fields:
//! `cargo run --example tng_component -- <FABLE_DIR>/data StartOakValeWest CTCLight`.

use std::fs::File;
use std::io::BufReader;

use fable_data::bbbb::BbbbFile;
use fable_data::tng::Tng;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [dir, level, component] = a.as_slice() else {
        return Err("usage: tng_component <data dir> <level> <component>".into());
    };
    let wad_path = std::path::Path::new(dir).join("Levels/FinalAlbion.wad");
    let wad = BbbbFile::open(&wad_path)?;
    let mut file = BufReader::new(File::open(&wad_path)?);
    let entry = wad
        .index
        .by_name(&format!("Data\\Levels\\FinalAlbion\\{level}.tng"))
        .ok_or("no such level")?;
    let tng = Tng::parse(&String::from_utf8_lossy(&entry.read_data(&mut file)?))?;
    for thing in &tng.things {
        let Some(c) = thing.component(component) else {
            continue;
        };
        println!(
            "{} {} [{}] at {:?}",
            thing.kind,
            thing.definition().unwrap_or("?"),
            thing.section,
            thing.position()
        );
        for f in &c.fields {
            println!("    {} = {}", f.key, f.value);
        }
    }
    Ok(())
}
