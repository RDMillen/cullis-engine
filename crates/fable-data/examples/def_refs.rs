//! Lists definitions whose records hold another definition's index or name hash:
//! `cargo run --example def_refs -- <FABLE_DIR>/data ENVIRONMENT_OAKVALE`.

use fable_data::defs::{Defs, Names, name_hash};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [dir, name] = a.as_slice() else {
        return Err("usage: def_refs <data dir> <definition>".into());
    };
    let dir = std::path::Path::new(dir).join("CompiledDefs");
    let names = Names::parse(&std::fs::read(dir.join("names.bin"))?)?;
    let defs = Defs::parse(&std::fs::read(dir.join("game.bin"))?, &names)?;
    let index = defs
        .defs
        .iter()
        .position(|d| d.name.as_deref() == Some(name.as_str()))
        .ok_or("no such definition")?;
    let hash = name_hash(name);
    println!("{name}: index {index}, hash {hash:08x}");
    for (i, d) in defs.defs.iter().enumerate() {
        for at in 0..d.record.len().saturating_sub(3) {
            let v = u32::from_le_bytes(d.record[at..at + 4].try_into().unwrap());
            let what = if v == hash {
                "hash"
            } else if v as usize == index {
                "index"
            } else {
                continue;
            };
            let before = &d.record[at.saturating_sub(8)..at];
            println!(
                "  {what:5} in {} #{i} ({}) at {at:#x}, before {before:02x?}",
                d.name.as_deref().unwrap_or("?"),
                defs.type_name(d)
            );
        }
    }
    Ok(())
}
