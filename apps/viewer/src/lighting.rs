//! Light by time of day, from the region's environment theme and the lighting
//! colour table (`fable_data::environment`): the sun's colour and direction,
//! the ambient light, and the sky's textures and tint.

use std::path::Path;

use fable_data::defs::Defs;
use fable_data::environment::{Environment, LightingTable, fields, rows};
use glam::Vec3;

/// The light at one moment. Colours are the table's values (gamma space, 0..1);
/// the shaders add sun and ambient there, then convert to linear, as old
/// fixed-function lighting multiplied texture and light in gamma space.
#[derive(Debug, Clone, PartialEq)]
pub struct Light {
    /// Towards the sun (or the moon at night), world space.
    pub sun_dir: Vec3,
    pub sun: [f32; 3],
    pub ambient: [f32; 3],
    /// The sky's tint (linear) and its textures (`GBANK_MAIN_PC` ids) with
    /// weights adding up to 1; empty for no sky (a plain colour).
    pub sky_tint: [f32; 3],
    pub sky: Vec<(u32, f32)>,
}

impl Default for Light {
    /// The look before lighting: a fixed sun and grey light, a plain sky.
    fn default() -> Self {
        Self {
            sun_dir: Vec3::new(-0.4, -0.5, 0.75).normalize(),
            sun: [0.55; 3],
            ambient: [0.45; 3],
            sky_tint: [1.0; 3],
            sky: Vec::new(),
        }
    }
}

/// A region's environment themes and the colour table.
pub struct Lighting {
    table: LightingTable,
    /// Outdoors, and inside buildings, with their definition names.
    pub outside: (String, Environment),
    pub inside: Option<(String, Environment)>,
}

impl Lighting {
    /// The themes of region definition `region_def`; `outside` names a theme to
    /// use instead of the region's own outdoor one (the burning village).
    pub fn load(
        data_dir: &Path,
        defs: &Defs,
        region_def: &str,
        outside: Option<&str>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let table = LightingTable::parse(&std::fs::read(
            data_dir.join("LightingTable/lighting_colours.tga"),
        )?)?;
        let region = defs.get(region_def).ok_or("no region definition")?;
        let theme = |index: u32| -> Option<(String, Environment)> {
            let def = defs.defs.get(index as usize)?;
            let env = Environment::parse(&def.record).ok()?;
            Some((def.name.clone()?, env))
        };
        let named = |name: &str| -> Option<(String, Environment)> {
            let def = defs.get(name)?;
            Some((name.to_owned(), Environment::parse(&def.record).ok()?))
        };
        let own = region
            .value_after(fields::ENVIRONMENT_THEME)
            .and_then(theme);
        let internal = region
            .value_after(fields::INTERNAL_ENVIRONMENT_THEME)
            .and_then(theme);
        // One region (the Guild's inside) marks itself lit by its internal theme.
        let lit_inside = region.value_after(fields::REGION_LIGHTING) == Some(2);
        let outside = match (outside.and_then(named), lit_inside) {
            (Some(o), _) => Some(o),
            (None, true) => internal.clone(),
            (None, false) => own,
        }
        .ok_or("region has no environment theme")?;
        Ok(Self {
            table,
            outside,
            inside: internal,
        })
    }

    /// Every sky texture the themes use, for loading up front.
    pub fn sky_textures(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = std::iter::once(&self.outside)
            .chain(&self.inside)
            .flat_map(|(_, env)| &env.keyframes)
            .flat_map(|k| [k.u32(fields::SKY_TEXTURE_0), k.u32(fields::SKY_TEXTURE_1)])
            .flatten()
            .filter(|&id| id != 0)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// The light at `hour`, outdoors or inside a building.
    pub fn at(&self, hour: f32, indoors: bool) -> Light {
        let env = match (&self.inside, indoors) {
            (Some((_, inside)), true) => inside,
            _ => &self.outside.1,
        };
        let colour =
            |row: usize, fallback: [f32; 3]| env.colour(&self.table, row, hour).unwrap_or(fallback);
        let ambient = colour(rows::AMBIENT, [0.45; 3]);
        // Indoor themes leave the sun's row unused: no sun.
        let sun = colour(rows::SUN, [0.0; 3]);
        let tint = colour(rows::ZENITH, [1.0; 3]);
        Light {
            sun_dir: sun_direction(hour),
            sun,
            ambient,
            sky_tint: tint.map(|c| c.powf(2.2)),
            sky: sky_weights(env, hour),
        }
    }
}

/// Towards the sun at `hour`: rising in the east at 06:00, highest (in the
/// south) at noon, setting in the west at 18:00. At night the same arc stands
/// for the moon. Never quite flat, so slopes keep some shape. A guess: the
/// themes' `SunHeight` and `MoonHeight` (0.2 and 0.9) are not used yet.
pub fn sun_direction(hour: f32) -> Vec3 {
    let a = ((hour - 6.0) / 12.0 * std::f32::consts::PI).rem_euclid(std::f32::consts::PI);
    Vec3::new(a.cos(), -0.45, 0.2 + 0.8 * a.sin()).normalize()
}

/// The sky textures at `hour` and their weights: each keyframe blends its
/// `SkyTexture0` and `SkyTexture1` by its blend field, and the two keyframes
/// either side of `hour` are blended by time.
fn sky_weights(env: &Environment, hour: f32) -> Vec<(u32, f32)> {
    const BLEND: u32 = 0x1574_0d29;
    let Some((a, b, t)) = env.between(hour) else {
        return Vec::new();
    };
    let mut weights: Vec<(u32, f32)> = Vec::new();
    for (k, share) in [(a, 1.0 - t), (b, t)] {
        let key = &env.keyframes[k];
        let blend = key.f32(BLEND).unwrap_or(0.0).clamp(0.0, 1.0);
        for (field, w) in [
            (fields::SKY_TEXTURE_0, 1.0 - blend),
            (fields::SKY_TEXTURE_1, blend),
        ] {
            let Some(id) = key.u32(field).filter(|&id| id != 0) else {
                continue;
            };
            match weights.iter_mut().find(|(i, _)| *i == id) {
                Some((_, sum)) => *sum += w * share,
                None => weights.push((id, w * share)),
            }
        }
    }
    weights.retain(|(_, w)| *w > 0.001);
    let total: f32 = weights.iter().map(|(_, w)| w).sum();
    if total > 0.0 {
        for (_, w) in &mut weights {
            *w /= total;
        }
    }
    weights
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sun_rises_east_and_sets_west() {
        let dawn = sun_direction(6.0);
        let noon = sun_direction(12.0);
        let dusk = sun_direction(18.0 - 1e-3);
        assert!(dawn.x > 0.8 && dusk.x < -0.8, "{dawn} {dusk}");
        assert!(noon.z > 0.85 && noon.y < 0.0, "high in the south: {noon}");
        assert!(dawn.z > 0.1, "never flat: {dawn}");
    }
}
