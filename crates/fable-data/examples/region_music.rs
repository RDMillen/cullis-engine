//! For each region definition, prints the values of fields whose names mention
//! music, atmosphere or sound: `cargo run --example region_music -- <FABLE_DIR>/data`.

use fable_data::defs::{Defs, Names};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("usage: region_music <data dir>")?,
    )
    .join("CompiledDefs");
    let names = Names::parse(&std::fs::read(dir.join("names.bin"))?)?;
    let defs = Defs::parse(&std::fs::read(dir.join("game.bin"))?, &names)?;
    let fields: Vec<&str> = names
        .iter()
        .map(|(_, n)| n)
        .filter(|n| {
            let u = n.to_ascii_uppercase();
            (u.contains("MUSIC")
                || u.contains("ATMOS")
                || u.contains("SOUND")
                || u.contains("REVERB"))
                && !n.contains(' ')
                && n.chars().any(|c| c.is_ascii_lowercase())
        })
        .collect();
    println!("candidate fields: {fields:?}");
    // Music sets as atmos_types.h numbers them (MUSIC_SET_NULL = 1), keyed by a
    // word in the region definition's name.
    let expected: &[(&str, u32)] = &[
        ("BANDIT_CAMP", 10),
        ("BOWERSTONE", 11),
        ("DARKWOOD", 13),
        ("GRAVEYARD", 14),
        ("GREATWOOD", 16),
        ("LOOKOUT", 18),
        ("OAK_VALE", 19),
        ("WITCHWOOD", 20),
        ("PRISON", 25),
        ("HOOK_COAST", 26),
        ("KNOTHOLE", 27),
        ("GIBBET", 29),
    ];
    let mut score: std::collections::HashMap<u32, (usize, usize)> = Default::default();
    for d in &defs.defs {
        let Some(name) = &d.name else { continue };
        if !name.starts_with("REGION_") {
            continue;
        }
        let Some(&(_, want)) = expected.iter().find(|(k, _)| name.contains(k)) else {
            continue;
        };
        let r = &d.record;
        let mut seen = std::collections::HashSet::new();
        for i in 0..r.len().saturating_sub(7) {
            let field = u32::from_le_bytes(r[i..i + 4].try_into().unwrap());
            let value = u32::from_le_bytes(r[i + 4..i + 8].try_into().unwrap());
            if !seen.insert(field) {
                continue;
            }
            let e = score.entry(field).or_default();
            e.1 += 1;
            if value == want {
                e.0 += 1;
            }
        }
    }
    let mut best: Vec<_> = score.into_iter().filter(|(_, (hit, _))| *hit > 2).collect();
    best.sort_by_key(|(_, (hit, _))| std::cmp::Reverse(*hit));
    for (field, (hit, of)) in best.iter().take(5) {
        println!("field {field:#010x}: {hit} of {of} expected music sets");
    }
    for d in &defs.defs {
        let Some(name) = &d.name else { continue };
        if !name.starts_with("REGION_") {
            continue;
        }
        let found: Vec<String> = fields
            .iter()
            .filter_map(|f| {
                let v = d.values_after(f);
                (!v.is_empty()).then(|| format!("{f}={v:?}"))
            })
            .collect();
        if !found.is_empty() {
            println!("{name}: {}", found.join(" "));
        }
    }
    Ok(())
}
