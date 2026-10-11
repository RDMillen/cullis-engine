//! Prints a mesh's levels of detail, materials and chunks (vertex count, height
//! range, sub-meshes with their material).
//!
//! Usage: `cargo run --example mesh_dump -- <FABLE_DIR>/data <mesh name or id>`

use std::fs::File;
use std::io::BufReader;

use fable_data::big::BigFile;
use fable_data::mesh::{Mesh, SKINNED_MESH_TYPE};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(dir), Some(want)) = (args.next(), args.next()) else {
        return Err("usage: mesh_dump <data dir> <mesh name or id>".into());
    };
    let mut file = BufReader::new(File::open(
        std::path::Path::new(&dir).join("graphics/graphics.big"),
    )?);
    let big = BigFile::read(&mut file)?;
    let index = big
        .bank("MBANK_ALLMESHES")
        .ok_or("no bank")?
        .read_index(&mut file)?;
    let entry = index
        .entries
        .iter()
        .find(|e| e.name == want || e.id.to_string() == want)
        .ok_or("no such mesh")?;
    let data = entry.read_data(&mut file)?;
    let mesh = if entry.entry_type == SKINNED_MESH_TYPE || std::env::var_os("SKINNED").is_some() {
        Mesh::parse_skinned(&entry.info, &data)?
    } else {
        Mesh::parse_static(&entry.info, &data)?
    };
    println!(
        "{} (id {}, type {}): {} bytes, LOD sizes {:?}",
        entry.name,
        entry.id,
        entry.entry_type,
        data.len(),
        mesh.info.lod_sizes
    );
    for (l, lod) in mesh.lods.iter().enumerate() {
        let names: Vec<_> = lod.materials.iter().map(|m| m.name.as_str()).collect();
        println!("LOD {l}: materials {names:?}");
        for (c, chunk) in lod.chunks.iter().enumerate() {
            let p = chunk.positions();
            let (lo, hi) = p.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| {
                (lo.min(v[2]), hi.max(v[2]))
            });
            let subs: Vec<String> = chunk
                .sub_meshes
                .iter()
                .map(|s| {
                    format!(
                        "{}x{}",
                        lod.materials
                            .get(s.material as usize)
                            .map_or("?", |m| m.name.as_str()),
                        chunk.triangles(s).len()
                    )
                })
                .collect();
            println!(
                "  chunk {c}: node {}, format {}, {} vertices, z {lo:.0}..{hi:.0}, physics blocks {}, {}",
                chunk.node,
                chunk.vertex_format,
                chunk.vertex_count,
                chunk.physics.len(),
                subs.join(", ")
            );
        }
    }
    Ok(())
}
