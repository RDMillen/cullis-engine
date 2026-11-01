//! For a set of strike animations, how far each one's last pose is from each
//! one's first, on the young Hero's skeleton (summed bone rotation angles,
//! radians): a strike whose start matches another's end follows it in a chain.
//!
//! Usage: `cargo run --release -p engine-world --example strike_chains -- <FABLE_DIR>/data NAME...`

use engine_core::assets::{Archive, AssetCache};
use engine_world::anim::{AnimBank, pose};
use fable_data::mesh::Mesh;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().ok_or("usage")?);
    let names: Vec<String> = args.collect();
    let mut assets = AssetCache::new(&dir);
    let index = assets.index(Archive::Meshes)?;
    let hero = index.by_name("MESH_YOUNGHERO_02").ok_or("no hero mesh")?.id;
    let (entry, data) = assets.read(Archive::Meshes, hero)?;
    let mesh = Mesh::parse_skinned(&entry.info, &data)?;
    let skeleton = mesh.skeleton().ok_or("no skeleton")?.clone();
    let mut bank = AnimBank::open(&assets)?;
    let clips: Vec<_> = names
        .iter()
        .map(|n| {
            let id = bank.id(n).ok_or(format!("no {n}"))?;
            bank.clip(id, &skeleton).map_err(|e| e.to_string())
        })
        .collect::<Result<_, _>>()?;
    // Rotation of every bone in model space at a time: the pose matrices'
    // upper 3x3 (bind to posed), so differences between poses compare.
    let rotations = |c: &engine_world::anim::Clip, t: f32| -> Vec<glam::Quat> {
        pose(&skeleton, Some(c), t, &[])
            .iter()
            .map(|m| glam::Quat::from_mat4(m).normalize())
            .collect()
    };
    let starts: Vec<_> = clips.iter().map(|c| rotations(c, 0.0)).collect();
    let ends: Vec<_> = clips
        .iter()
        .map(|c| rotations(c, c.last_frame_time()))
        .collect();
    print!("{:40}", "end of \\ start of");
    for n in &names {
        print!(" {:>8}", n.trim_start_matches("ANIM_HERO_"));
    }
    println!();
    for (a, end) in ends.iter().enumerate() {
        print!("{:40}", names[a]);
        for start in &starts {
            let d: f32 = end
                .iter()
                .zip(start)
                .map(|(p, q)| p.angle_between(*q))
                .sum();
            print!(" {d:8.2}");
        }
        println!();
    }
    Ok(())
}
