//! Lists a level's things whose definition or script name contains a word,
//! with kind, quest section, position and script name:
//! `cargo run --example tng_things -- <FABLE_DIR>/data HeroGuildComplexInside DUMMY`.

use std::fs::File;
use std::io::BufReader;

use fable_data::bbbb::BbbbFile;
use fable_data::tng::Tng;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [dir, level, word] = a.as_slice() else {
        return Err("usage: tng_things <data dir> <level> <word>".into());
    };
    let wad_path = std::path::Path::new(dir).join("Levels/FinalAlbion.wad");
    let wad = BbbbFile::open(&wad_path)?;
    let mut file = BufReader::new(File::open(&wad_path)?);
    let entry = wad
        .index
        .by_name(&format!("Data\\Levels\\FinalAlbion\\{level}.tng"))
        .ok_or("no such level")?;
    let tng = Tng::parse(&String::from_utf8_lossy(&entry.read_data(&mut file)?))?;
    for t in &tng.things {
        let def = t.definition().unwrap_or("?");
        let script = t.script_name().unwrap_or("");
        if !(def.contains(word.as_str()) || script.contains(word.as_str())) {
            continue;
        }
        let p = t.position().unwrap_or_default();
        println!(
            "{:10} {def:40} [{}] at ({:.1}, {:.1}, {:.1}) {script}",
            t.kind, t.section, p[0], p[1], p[2]
        );
    }
    Ok(())
}
