//! Writes one definition's record bytes to a file:
//! `cargo run --example def_raw -- <FABLE_DIR>/data ENVIRONMENT_OAKVALE <out>`.

use fable_data::defs::{Defs, Names};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [dir, name, out] = a.as_slice() else {
        return Err("usage: def_raw <data dir> <definition> <out>".into());
    };
    let dir = std::path::Path::new(dir).join("CompiledDefs");
    let names = Names::parse(&std::fs::read(dir.join("names.bin"))?)?;
    let defs = Defs::parse(&std::fs::read(dir.join("game.bin"))?, &names)?;
    let def = defs.get(name).ok_or("no such definition")?;
    std::fs::write(out, &def.record)?;
    Ok(())
}
