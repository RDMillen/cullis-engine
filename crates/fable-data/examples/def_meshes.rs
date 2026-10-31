//! Prints the meshes definitions are drawn with (`Defs::meshes_of`):
//! `cargo run --example def_meshes -- <FABLE_DIR>/data BUILDING_GUILD_LIBRARY_01`.

use fable_data::defs::{Defs, Names};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (dir, wanted) = a
        .split_first()
        .ok_or("usage: def_meshes <data dir> <definition>...")?;
    let dir = std::path::Path::new(dir).join("CompiledDefs");
    let names = Names::parse(&std::fs::read(dir.join("names.bin"))?)?;
    let defs = Defs::parse(&std::fs::read(dir.join("game.bin"))?, &names)?;
    for name in wanted {
        let def = defs.get(name).ok_or("no such definition")?;
        println!("{name}: {:?}", defs.meshes_of(def));
    }
    Ok(())
}
