//! Lists definition types and the definitions whose type or name contains a word:
//! `cargo run --example def_names -- <FABLE_DIR>/data LIGHT`.

use fable_data::defs::{Defs, Names};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().ok_or("usage: def_names <data dir> <word>")?)
        .join("CompiledDefs");
    let word = args
        .next()
        .ok_or("usage: def_names <data dir> <word>")?
        .to_ascii_uppercase();
    let names = Names::parse(&std::fs::read(dir.join("names.bin"))?)?;
    let defs = Defs::parse(&std::fs::read(dir.join("game.bin"))?, &names)?;
    for d in &defs.defs {
        let ty = &defs.types[d.def_type].name;
        let name = d.name.as_deref().unwrap_or("?");
        if ty.to_ascii_uppercase().contains(&word) || name.to_ascii_uppercase().contains(&word) {
            println!("{ty:28} {name} ({} bytes)", d.record.len());
        }
    }
    Ok(())
}
