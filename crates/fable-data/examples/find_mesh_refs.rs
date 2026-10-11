//! For a definition and each of its components, prints every byte offset holding a
//! skinned or static mesh id, with the field hash before it (named if known).
//!
//! Usage: `cargo run --example find_mesh_refs -- <FABLE_DIR>/data <definition>`

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;

use fable_data::big::BigFile;
use fable_data::defs::{Defs, Names, name_hash};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(dir), Some(name)) = (args.next(), args.next()) else {
        return Err("usage: find_mesh_refs <data dir> <definition>".into());
    };
    let dir = std::path::PathBuf::from(dir);
    let names = Names::parse(&std::fs::read(dir.join("CompiledDefs/names.bin"))?)?;
    let defs = Defs::parse(&std::fs::read(dir.join("CompiledDefs/game.bin"))?, &names)?;
    let by_hash: HashMap<u32, &str> = names.iter().collect();
    let mut file = BufReader::new(File::open(dir.join("graphics/graphics.big"))?);
    let big = BigFile::read(&mut file)?;
    let index = big
        .bank("MBANK_ALLMESHES")
        .ok_or("no bank")?
        .read_index(&mut file)?;
    let meshes: HashMap<u32, (String, u32)> = index
        .entries
        .iter()
        .filter(|e| [1, 5].contains(&e.entry_type))
        .map(|e| (e.id, (e.name.clone(), e.entry_type)))
        .collect();
    let def = defs.get(&name).ok_or("no such definition")?;
    let mut todo = vec![(name.clone(), def)];
    // Components: (type hash, u32 index, u32) triples in the entity record.
    let r = &def.record;
    for at in 0..r.len().saturating_sub(8) {
        let h = u32::from_le_bytes(r[at..at + 4].try_into().unwrap());
        let Some(type_name) = by_hash.get(&h) else {
            continue;
        };
        let i = u32::from_le_bytes(r[at + 4..at + 8].try_into().unwrap()) as usize;
        if let Some(c) = defs.defs.get(i).filter(|c| defs.type_name(c) == *type_name) {
            todo.push((format!("{type_name} #{i}"), c));
        }
    }
    let _ = name_hash("Graphic");
    for (label, d) in todo {
        let r = &d.record;
        for at in 4..r.len().saturating_sub(3) {
            let v = u32::from_le_bytes(r[at..at + 4].try_into().unwrap());
            if let Some((mesh, ty)) = meshes.get(&v).filter(|_| v > 100) {
                let before = u32::from_le_bytes(r[at - 4..at].try_into().unwrap());
                let field = by_hash.get(&before).copied().unwrap_or("?");
                println!("{label}: @{at:#x} {v} {mesh} (type {ty}), after {before:#010x} {field}");
            }
        }
    }
    Ok(())
}
