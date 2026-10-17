//! Lists the definitions that have a component of a type, with that
//! component's record: `cargo run --example def_components -- <FABLE_DIR>/data CLightDef`.

use fable_data::defs::{Defs, Names};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [dir, component] = a.as_slice() else {
        return Err("usage: def_components <data dir> <component type>".into());
    };
    let dir = std::path::Path::new(dir).join("CompiledDefs");
    let names = Names::parse(&std::fs::read(dir.join("names.bin"))?)?;
    let defs = Defs::parse(&std::fs::read(dir.join("game.bin"))?, &names)?;
    for d in &defs.defs {
        let Some(c) = d
            .component(component)
            .and_then(|i| defs.defs.get(i))
            .filter(|c| defs.type_name(c) == component)
        else {
            continue;
        };
        println!(
            "{} ({}): {:02x?}",
            d.name.as_deref().unwrap_or("?"),
            defs.type_name(d),
            c.record
        );
    }
    Ok(())
}
