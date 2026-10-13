//! Prints each region's music set and file, and each sound theme's atmosphere
//! and bank: `cargo run --example sound_map -- <FABLE_DIR>/data`.

use fable_data::atmos::{self, Enums};
use fable_data::defs::{Defs, Names};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("usage: sound_map <data dir>")?,
    );
    let compiled = dir.join("CompiledDefs");
    let names = Names::parse(&std::fs::read(compiled.join("names.bin"))?)?;
    let defs = Defs::parse(&std::fs::read(compiled.join("game.bin"))?, &names)?;
    let enums = Enums::open(&dir)?;
    let sound = dir.join("Sound");
    for d in &defs.defs {
        let Some(name) = &d.name else { continue };
        if let Some(set) = name
            .starts_with("REGION_")
            .then(|| atmos::region_music_set(d))
            .flatten()
        {
            let set_name = enums.name("EMusicSetType", i64::from(set)).unwrap_or("?");
            let file = atmos::music_file(&sound, set_name);
            println!(
                "{name}: {set} {set_name} -> {:?}",
                file.map(|f| f.file_name().unwrap().to_owned())
            );
        }
        if let Some(a) = name
            .starts_with("SOUND_THEME_")
            .then(|| atmos::theme_atmos(d))
            .flatten()
        {
            let a_name = enums.name("EAtmosType", i64::from(a)).unwrap_or("?");
            let file = atmos::atmos_bank(&sound, a_name);
            println!(
                "{name}: {a} {a_name} -> {:?}",
                file.map(|f| f.file_name().unwrap().to_owned())
            );
        }
    }
    Ok(())
}
