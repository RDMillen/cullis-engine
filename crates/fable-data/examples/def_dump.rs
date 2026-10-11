//! Prints a `game.bin` definition's record: every byte offset holding the hash of
//! a known name (a field, type or definition), with the bytes after it as a `u32`
//! and an `f32`. Records are not word-aligned, so every offset is tried.
//!
//! Usage: `cargo run --example def_dump -- <FABLE_DIR>/data CREATURE_HERO_CHILD`
//! (a number instead of a name picks a definition by its global index).

use std::collections::HashMap;

use fable_data::defs::{Defs, Names};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(dir), Some(name)) = (args.next(), args.next()) else {
        return Err("usage: def_dump <data dir> <definition name or index>".into());
    };
    let dir = std::path::Path::new(&dir).join("CompiledDefs");
    let names = Names::parse(&std::fs::read(dir.join("names.bin"))?)?;
    let defs = Defs::parse(&std::fs::read(dir.join("game.bin"))?, &names)?;
    let def = match name.parse::<usize>() {
        Ok(i) => defs.defs.get(i),
        Err(_) => defs.get(&name),
    }
    .ok_or("no such definition")?;
    let by_hash: HashMap<u32, &str> = names.iter().collect();
    let r = &def.record;
    println!(
        "{}: type {}, {} bytes",
        def.name.as_deref().unwrap_or(&name),
        defs.type_name(def),
        r.len()
    );
    if r.len() <= 256 {
        for (i, line) in r.chunks(16).enumerate() {
            println!("{:5x}  {:02x?}", i * 16, line);
        }
    }
    if std::env::var_os("DEF_FLOATS").is_some() {
        for at in 0..r.len().saturating_sub(3) {
            let f = f32::from_le_bytes(r[at..at + 4].try_into().unwrap());
            if (0.3..0.95).contains(&f) {
                println!(
                    "{at:5x}  float {f}  context {:02x?}",
                    &r[at.saturating_sub(4)..(at + 8).min(r.len())]
                );
            }
        }
    }
    let word = |at: usize| {
        r.get(at..at + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    for at in 0..r.len().saturating_sub(3) {
        let Some(field) = word(at).and_then(|h| by_hash.get(&h)) else {
            continue;
        };
        let after: Vec<String> = (0..3)
            .filter_map(|k| word(at + 4 + 4 * k))
            .map(|w| {
                let f = f32::from_bits(w);
                if f.is_normal() && (1e-3..1e6).contains(&f.abs()) {
                    format!("{w:#x} ({f})")
                } else {
                    format!("{w:#x}")
                }
            })
            .collect();
        println!("{at:5x}  {field:<32} {}", after.join("  "));
    }
    Ok(())
}
