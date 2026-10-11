//! Prints a creature's animation table (`CAppearanceDef`): slot, modifier and the
//! animation's bank entry name.
//!
//! Usage: `cargo run --example anim_slots -- <FABLE_DIR>/data CREATURE_HERO_CHILD [filter]`

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;

use fable_data::big::BigFile;
use fable_data::defs::{Defs, Names};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(dir), Some(name)) = (args.next(), args.next()) else {
        return Err("usage: anim_slots <data dir> <creature> [filter]".into());
    };
    let filter = args.next().unwrap_or_default().to_ascii_uppercase();
    let dir = std::path::PathBuf::from(dir);
    let names = Names::parse(&std::fs::read(dir.join("CompiledDefs/names.bin"))?)?;
    let defs = Defs::parse(&std::fs::read(dir.join("CompiledDefs/game.bin"))?, &names)?;
    let mut file = BufReader::new(File::open(dir.join("graphics/graphics.big"))?);
    let big = BigFile::read(&mut file)?;
    let index = big
        .bank("MBANK_ALLMESHES")
        .ok_or("no bank")?
        .read_index(&mut file)?;
    let bank: HashMap<u32, String> = index.entries.into_iter().map(|e| (e.id, e.name)).collect();
    let def = defs.get(&name).ok_or("no such definition")?;
    let slots = defs.animation_slots(def, &names);
    println!("{name}: {} animation slots", slots.len());
    for s in slots.iter().filter(|s| s.slot.contains(&filter)) {
        println!(
            "  {:<36} {:<20} {}",
            s.slot,
            s.modifier.as_deref().unwrap_or("-"),
            bank.get(&s.animation).map_or("?", String::as_str)
        );
    }
    Ok(())
}
