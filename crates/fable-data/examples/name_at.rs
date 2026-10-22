//! Prints the names at byte offsets of `names.bin`:
//! `cargo run --example name_at -- <FABLE_DIR>/data 368645 344371`.

use fable_data::defs::Names;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().ok_or("usage: name_at <data dir> <offset>...")?);
    let names = Names::parse(&std::fs::read(dir.join("CompiledDefs/names.bin"))?)?;
    for a in args {
        let offset: u32 = a.parse()?;
        println!("{offset} ({offset:#x}): {:?}", names.at(offset));
    }
    Ok(())
}
