//! Prints regions from `FinalAlbion.wld`: the maps each contains and sees, and the
//! Tng things in its maps whose definition or kind mentions `filter`.
//!
//! Usage: `cargo run --example region_dump -- <FABLE_DIR>/data <filter> <region>...`

use std::fs::File;
use std::io::BufReader;

use fable_data::bbbb::BbbbFile;
use fable_data::tng::Tng;
use fable_data::wld::Wld;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().ok_or("usage")?);
    let filter = args.next().ok_or("usage")?.to_ascii_uppercase();
    let wld = Wld::open(dir.join("Levels/FinalAlbion.wld"))?;
    let wad_path = dir.join("Levels/FinalAlbion.wad");
    let wad = BbbbFile::open(&wad_path)?;
    let mut file = BufReader::new(File::open(&wad_path)?);
    if let Some(uids) = filter.strip_prefix("UID:") {
        let uids: Vec<u64> = uids
            .split(',')
            .map(|u| u.parse())
            .collect::<Result<_, _>>()?;
        for map in &wld.maps {
            let tng_name = format!("Data\\Levels\\{}", map.level_name.replace(".lev", ".tng"));
            let Some(entry) = wad.index.by_name(&tng_name) else {
                continue;
            };
            let text: String = entry
                .read_data(&mut file)?
                .iter()
                .map(|&b| b as char)
                .collect();
            for t in &Tng::parse(&text)?.things {
                if t.uid().is_some_and(|u| uids.contains(&u)) {
                    let p = t.position().unwrap_or_default();
                    let region = wld
                        .regions
                        .iter()
                        .filter(|r| r.contains_maps.contains(&map.level_name))
                        .map(|r| r.name.as_str())
                        .collect::<Vec<_>>();
                    println!(
                        "{:?}: {} {} in {} {region:?} at ({:.1}, {:.1}, {:.1}) fields {:?}",
                        t.uid(),
                        t.kind,
                        t.definition().unwrap_or("?"),
                        map.level_name,
                        map.x as f32 + p[0],
                        map.y as f32 + p[1],
                        p[2],
                        t.components
                            .iter()
                            .map(|c| (
                                c.name.clone(),
                                c.fields
                                    .iter()
                                    .map(|f| format!("{}={}", f.key, f.value))
                                    .collect::<Vec<_>>()
                            ))
                            .filter(|(n, _)| !n.contains("Physics") && !n.contains("Editor"))
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
        return Ok(());
    }
    for name in args {
        let region = wld.region(&name).ok_or("no region")?;
        println!(
            "== {} ({}): contains {:?}",
            region.name, region.display_name, region.contains_maps
        );
        println!("   sees {:?}", region.sees_maps);
        for level in &region.contains_maps {
            let Some(map) = wld.map(level) else { continue };
            println!("   map {level} at ({}, {})", map.x, map.y);
            let tng_name = format!("Data\\Levels\\{}", level.replace(".lev", ".tng"));
            let Some(entry) = wad.index.by_name(&tng_name) else {
                continue;
            };
            let text: String = entry
                .read_data(&mut file)?
                .iter()
                .map(|&b| b as char)
                .collect();
            for t in &Tng::parse(&text)?.things {
                let def = t.definition().unwrap_or("?");
                if def.to_ascii_uppercase().contains(&filter)
                    || t.kind.to_ascii_uppercase().contains(&filter)
                {
                    let p = t.position().unwrap_or_default();
                    println!(
                        "      {} {} at ({:.1}, {:.1}, {:.1})",
                        t.kind,
                        def,
                        map.x as f32 + p[0],
                        map.y as f32 + p[1],
                        p[2]
                    );
                    if std::env::var_os("FULL").is_some() {
                        for f in &t.fields {
                            println!("         {} = {}", f.key, f.value);
                        }
                        for c in &t.components {
                            println!("         [{}]", c.name);
                            for f in &c.fields {
                                println!("            {} = {}", f.key, f.value);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
