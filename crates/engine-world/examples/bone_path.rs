//! Where named bones of the young Hero are (model space, mesh units) through an
//! animation: `cargo run --release -p engine-world --example bone_path --
//! <FABLE_DIR>/data ANIM_HERO_COMBAT_IDLE_1_1 "Bip01 R Hand" WEAPON_FOCUS_01`.

use engine_core::assets::{Archive, AssetCache};
use engine_world::anim::{AnimBank, bind_origin, pose};
use fable_data::mesh::Mesh;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().ok_or("usage")?);
    let anim = args.next().ok_or("usage")?;
    let bones: Vec<String> = args.collect();
    let mut assets = AssetCache::new(&dir);
    let hero = assets
        .index(Archive::Meshes)?
        .by_name("MESH_YOUNGHERO_02")
        .ok_or("no hero mesh")?
        .id;
    let (entry, data) = assets.read(Archive::Meshes, hero)?;
    let mesh = Mesh::parse_skinned(&entry.info, &data)?;
    let skeleton = mesh.skeleton().ok_or("no skeleton")?.clone();
    let mut bank = AnimBank::open(&assets)?;
    let id = bank.id(&anim).ok_or("no animation")?;
    let clip = bank.clip(id, &skeleton)?;
    let indices: Vec<usize> = bones
        .iter()
        .map(|n| {
            skeleton
                .bones
                .iter()
                .position(|b| &b.name == n)
                .ok_or(format!("no bone {n}"))
        })
        .collect::<Result<_, _>>()?;
    for step in 0..=4 {
        let t = clip.last_frame_time() * step as f32 / 4.0;
        let posed = pose(&skeleton, Some(&clip), t, &[]);
        print!("{t:5.2}");
        for &i in &indices {
            let p = posed[i].transform_point3(bind_origin(&skeleton.bones[i]));
            print!(
                "  {:>16} ({:7.1}, {:7.1}, {:7.1})",
                skeleton.bones[i].name, p.x, p.y, p.z
            );
        }
        println!();
    }
    Ok(())
}
