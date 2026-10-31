//! Lists definitions whose records hold any of the given mesh ids as a `u32`:
//! `cargo run --example mesh_id_refs -- <FABLE_DIR>/data 5894 5895`.

use fable_data::defs::{Defs, Names};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (dir, ids) = a
        .split_first()
        .ok_or("usage: mesh_id_refs <data dir> <id>...")?;
    let ids: Vec<u32> = ids.iter().map(|i| i.parse()).collect::<Result<_, _>>()?;
    let dir = std::path::Path::new(dir).join("CompiledDefs");
    let names = Names::parse(&std::fs::read(dir.join("names.bin"))?)?;
    let defs = Defs::parse(&std::fs::read(dir.join("game.bin"))?, &names)?;
    for (i, d) in defs.defs.iter().enumerate() {
        for at in 0..d.record.len().saturating_sub(3) {
            let v = u32::from_le_bytes(d.record[at..at + 4].try_into()?);
            if ids.contains(&v) {
                println!(
                    "{v} in {} #{i} ({}) at {at:#x}",
                    d.name.as_deref().unwrap_or("?"),
                    defs.type_name(d)
                );
            }
        }
    }
    Ok(())
}
