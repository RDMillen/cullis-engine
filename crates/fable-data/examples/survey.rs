//! Prints a Markdown survey of an install's banks and world layout.
//!
//! `FABLE_DIR=... cargo run --release -p fable-data --example survey`

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use fable_data::bbbb::BbbbFile;
use fable_data::big::BigFile;
use fable_data::wld::Wld;

fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_048_576.0)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = PathBuf::from(std::env::var_os("FABLE_DIR").ok_or("set FABLE_DIR")?).join("data");

    println!("## BIGB archives\n");
    println!("| File | Bank | Entries | Entry data | Types (type: count) |");
    println!("| --- | --- | --- | --- | --- |");
    let mut bigs = Vec::new();
    let mut dirs = vec![data.clone()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("big"))
            {
                bigs.push(path);
            }
        }
    }
    bigs.sort();
    for path in bigs {
        let mut file = BufReader::new(File::open(&path)?);
        let big = BigFile::read(&mut file)?;
        for bank in &big.banks {
            let index = bank.read_index(&mut file)?;
            let total: u64 = index.entries.iter().map(|e| u64::from(e.size)).sum();
            let types: Vec<String> = index
                .type_counts
                .iter()
                .map(|t| format!("{}: {}", t.entry_type, t.count))
                .collect();
            println!(
                "| `{}` | `{}` | {} | {} | {} |",
                path.strip_prefix(&data)?.display(),
                bank.name,
                index.entries.len(),
                mb(total),
                types.join(", ")
            );
        }
    }

    let wad = BbbbFile::open(data.join("Levels/FinalAlbion.wad"))?;
    let stb = BbbbFile::open(data.join("Levels/FinalAlbion_RT.stb"))?;
    let wld = Wld::open(data.join("Levels/FinalAlbion.wld"))?;
    let wad_size = |level: &str, ext: &str| -> u64 {
        let stem = level.strip_suffix(".lev").unwrap_or(level);
        wad.index
            .by_name(&format!("Data\\Levels\\{stem}.{ext}"))
            .map_or(0, |e| e.size.into())
    };

    println!("\n## BBBB banks\n");
    for (name, bank) in [("FinalAlbion.wad", &wad), ("FinalAlbion_RT.stb", &stb)] {
        let total: u64 = bank.index.entries.iter().map(|e| u64::from(e.size)).sum();
        let mut prefixes: BTreeMap<String, (usize, u64)> = BTreeMap::new();
        for e in &bank.index.entries {
            let prefix = match e.name.rfind("__") {
                Some(i) if e.name.starts_with("__") => e.name[..i + 2].to_owned(),
                _ => format!("*.{}", e.name.rsplit('.').next().unwrap_or("")),
            };
            let p = prefixes.entry(prefix).or_default();
            p.0 += 1;
            p.1 += u64::from(e.size);
        }
        let h = &bank.header;
        println!(
            "`{name}`: version {:#x}, unknown_a {:#x}, entry counts {}/{}, {} entries, {} of entry data.\n",
            h.version,
            h.unknown_a,
            h.entry_count_a,
            h.entry_count_b,
            bank.index.entries.len(),
            mb(total)
        );
        println!("| Name pattern | Entries | Size |\n| --- | --- | --- |");
        for (prefix, (count, size)) in &prefixes {
            println!("| `{prefix}` | {count} | {} |", mb(*size));
        }
        let mut largest: Vec<_> = bank.index.entries.iter().collect();
        largest.sort_by_key(|e| std::cmp::Reverse(e.size));
        println!("\nLargest entries:\n");
        for e in largest.iter().take(5) {
            println!("- `{}`: {}", e.name, mb(e.size.into()));
        }
        println!();
    }

    println!("## World\n");
    println!(
        "{} maps ({} loaded on player proximity, {} sea), {} regions, {} initial quests.\n",
        wld.maps.len(),
        wld.maps
            .iter()
            .filter(|m| m.loaded_on_player_proximity)
            .count(),
        wld.maps.iter().filter(|m| m.is_sea).count(),
        wld.regions.len(),
        wld.initial_quests.len()
    );

    let stb_size = |level: &str| -> u64 {
        stb.index
            .by_name(&format!("Data\\Levels\\{level}"))
            .map_or(0, |e| e.size.into())
    };
    let level_bytes = |levels: &[String]| -> u64 {
        levels
            .iter()
            .map(|l| wad_size(l, "lev") + wad_size(l, "tng") + stb_size(l))
            .sum()
    };

    println!(
        "### Largest regions by Lev + Tng + Stb size (contained maps plus maps seen from them)\n"
    );
    println!("| Region | Contains | Sees | Contained | Seen | Total |");
    println!("| --- | --- | --- | --- | --- | --- |");
    let mut regions: Vec<_> = wld
        .regions
        .iter()
        .map(|r| (r, level_bytes(&r.contains_maps), level_bytes(&r.sees_maps)))
        .collect();
    regions.sort_by_key(|&(_, c, s)| std::cmp::Reverse(c + s));
    for (r, c, s) in regions.iter().take(15) {
        println!(
            "| {} | {} | {} | {} | {} | {} |",
            r.name,
            r.contains_maps.len(),
            r.sees_maps.len(),
            mb(*c),
            mb(*s),
            mb(c + s)
        );
    }

    println!("\n### Oakvale regions\n");
    for name in [
        "StartOakVale",
        "StartOakvale_Filler",
        "OakBay",
        "OakvaleMemorialGarden",
    ] {
        let Some(r) = wld.region(name) else { continue };
        println!(
            "**{}** (`{}`, def `{}`)\n",
            r.name, r.display_name, r.region_def
        );
        println!(
            "| Map | Role | X | Y | Lev | Tng | Stb |\n| --- | --- | --- | --- | --- | --- | --- |"
        );
        for (role, levels) in [("contains", &r.contains_maps), ("sees", &r.sees_maps)] {
            for level in levels {
                let (x, y) = wld.map(level).map_or((0, 0), |m| (m.x, m.y));
                println!(
                    "| `{level}` | {role} | {x} | {y} | {} | {} | {} |",
                    mb(wad_size(level, "lev")),
                    mb(wad_size(level, "tng")),
                    mb(stb_size(level))
                );
            }
        }
        println!();
    }
    Ok(())
}
