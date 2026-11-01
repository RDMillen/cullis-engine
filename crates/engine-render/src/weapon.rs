//! The Hero's weapon, drawn in his hand or on his back: its mesh's grip
//! (`WEAPON_FOCUS_01`) is put on one of his mesh's sockets (`WEAPON_FOCUS_01` in
//! his right hand, `WEAPON_SCABBARD_01` on his back), which follows the bone
//! nearest it in his bind pose (the bone of the same name). In the hand the
//! weapon is turned end over end: put on the socket as it is, the stick hangs
//! down from his fist in the combat stance; turned, it stands up from it as a
//! held sword does. On the back it sits as it is, across his back and over his
//! shoulder (both by eye).

use engine_core::assets::{Archive, AssetCache};
use fable_data::mesh::{Mesh, Skeleton};
use glam::Mat4;

use crate::model::{self, ModelRenderer, dummy_matrix};

type Error = Box<dyn std::error::Error>;

/// A socket on the Hero: the bone it follows and its place in his bind pose.
#[derive(Debug, Clone, Copy)]
struct Socket {
    bone: usize,
    at: Mat4,
}

impl Socket {
    /// The dummy `name` on `skeleton`, following the bone nearest it.
    fn of(skeleton: &Skeleton, name: &str) -> Option<Self> {
        let dummy = skeleton.named_dummy(name)?;
        let at = dummy_matrix(dummy);
        let p = at.w_axis.truncate();
        let bone = skeleton
            .bones
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let d = |bone| engine_world::anim::bind_origin(bone).distance(p);
                d(*a).total_cmp(&d(*b))
            })?
            .0;
        Some(Self { bone, at })
    }
}

/// The weapon's mesh and where it goes on the Hero.
pub struct HeroWeapon {
    pub renderer: ModelRenderer,
    hand: Socket,
    back: Socket,
    /// The weapon mesh's space from its grip's.
    grip: Mat4,
    /// Where it is drawn this frame, if it is.
    pub transform: Option<Mat4>,
}

impl HeroWeapon {
    /// Loads mesh `name` (`MESH_HERO_STICK`) for the Hero whose skeleton is
    /// `hero`.
    pub fn load(
        assets: &AssetCache,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        hero: &Skeleton,
        name: &str,
    ) -> Result<Self, Error> {
        let mut assets = assets.clone();
        let id = assets
            .index(Archive::Meshes)?
            .by_name(name)
            .ok_or_else(|| format!("no mesh {name}"))?
            .id;
        let (entry, data) = assets.read(Archive::Meshes, id)?;
        let mesh = Mesh::parse_static(&entry.info, &data)?;
        let grip = mesh
            .skeleton()
            .and_then(|s| s.named_dummy("WEAPON_FOCUS_01"))
            .map_or(Mat4::IDENTITY, dummy_matrix);
        let hand = Socket::of(hero, "WEAPON_FOCUS_01").ok_or("no hand socket")?;
        let back = Socket::of(hero, "WEAPON_SCABBARD_01").ok_or("no back socket")?;
        for (what, s) in [("hand", hand), ("back", back)] {
            log::debug!(
                "{name} {what}: follows {} from {}",
                hero.bones[s.bone].name,
                s.at.w_axis.truncate()
            );
        }
        let model = model::static_model(&assets, id)?;
        Ok(Self {
            renderer: ModelRenderer::new(device, queue, &model),
            hand,
            back,
            grip: grip.inverse(),
            transform: None,
        })
    }

    /// Places the weapon for the Hero at `hero` (his model's transform) posed by
    /// `bones` (his skinning matrices), in his hand or on his back; `None` puts
    /// it nowhere.
    pub fn place(&mut self, hero: Mat4, bones: &[Mat4], in_hand: Option<bool>) {
        self.transform = in_hand.and_then(|hand| {
            let socket = if hand { self.hand } else { self.back };
            let bone = bones.get(socket.bone)?;
            let turn = if hand {
                Mat4::from_rotation_x(std::f32::consts::PI)
            } else {
                Mat4::IDENTITY
            };
            Some(hero * *bone * socket.at * turn * self.grip)
        });
    }
}
