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
    /// The two cloud layers (texture id, opacity; id 0 for none) and their
    /// colour (linear).
    pub clouds: [(u32, f32); 2],
    pub cloud_colour: [f32; 3],
    /// World to the sun's clip space for shadows, and the shadows' strength
    /// (0: none), one shadow map texel in uv, and depth bias; set per frame by
    /// the renderer.
    pub shadow_proj: glam::Mat4,
    pub shadow: [f32; 4],
    /// Point lights near the camera, two entries each: position and outer
    /// radius, then colour (times flicker and night dimming) and inner radius;
    /// `point_count` of them are used.
    pub points: [[f32; 4]; 2 * MAX_POINTS],
    pub point_count: u32,
    /// Scales the light, so it can go past 1 (overbright): with the table's
    /// colours alone (noon ambient about 60%), shaded sides look dull.
    pub brightness: f32,
    /// The sky's brightness (gamma space): the scene's by day, so the two
    /// match, easing to [`NIGHT_SKY`] of it at night (the night lift aside).
    pub sky_brightness: f32,
    /// The most light (after brightness) a surface may get, its colour kept:
    /// without it, sunlit faces reach about 2.8 and wash out.
    pub highlight_limit: f32,
}

/// The default highlight limit, by eye.
pub const HIGHLIGHT_LIMIT: f32 = 1.3;

/// Point lights the shaders take at once.
pub const MAX_POINTS: usize = 16;

/// The sky's share of the scene's brightness at night (by eye: the full share
/// turned night skies bright violet over a dark village).
pub const NIGHT_SKY: f32 = 0.75;

/// The default brightness, matched against the original (2.0 to 2.25 looks
/// right).
pub const BRIGHTNESS: f32 = 2.1;

impl Default for Light {
    /// The look before lighting: a fixed sun and grey light, a plain sky.
    fn default() -> Self {
        Self {
            sun_dir: Vec3::new(-0.4, -0.5, 0.75).normalize(),
            sun: [0.55; 3],
            ambient: [0.45; 3],
            sky_tint: [1.0; 3],
            sky: Vec::new(),
            clouds: [(0, 0.0); 2],
            cloud_colour: [0.0; 3],
            shadow_proj: glam::Mat4::IDENTITY,
            shadow: [0.0; 4],
            points: [[0.0; 4]; 2 * MAX_POINTS],
            point_count: 0,
            brightness: 1.0,
            sky_brightness: 1.0,
            highlight_limit: 1.0,
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
            .flat_map(|k| {
                [
                    fields::SKY_TEXTURE_0,
                    fields::SKY_TEXTURE_1,
                    fields::CLOUD_TEXTURE_0,
                    fields::CLOUD_TEXTURE_1,
                ]
                .map(|f| k.u32(f))
            })
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
        let tint = colour(rows::ZENITH, [1.0; 3]);
        let outdoor_sun = sun_direction(hour);
        let has_sun = env.colour(&self.table, rows::SUN, hour).is_some();
        // Indoors, row 0. Outdoors, the average of the warm row 0 and the cool
        // row 1: either alone tints shade orange or lavender (by eye).
        let warm = colour(rows::AMBIENT, [0.45; 3]);
        let ambient = match has_sun {
            true => {
                let cool = colour(rows::SKY_FILL, [0.45; 3]);
                std::array::from_fn(|i| (warm[i] + cool[i]) * 0.5)
            }
            false => warm,
        };
        // Indoor themes leave the sun's row unused. Their light comes in
        // through windows and doors instead: row 7, from high up on the sun's
        // side, at half strength (by eye; the brightness factor lifts it too) so walls facing away keep some shadow.
        let (sun, sun_dir) = match env.colour(&self.table, rows::SUN, hour) {
            Some(sun) => (sun, outdoor_sun),
            None => {
                let key = colour(rows::KEY, [0.0; 3]).map(|c| c * INDOOR_KEY);
                let side = outdoor_sun.truncate().normalize_or(glam::Vec2::NEG_Y);
                (key, (side * 0.6).extend(1.0).normalize())
            }
        };
        Light {
            sun_dir,
            sun,
            ambient,
            sky_tint: tint.map(|c| c.powf(2.2)),
            sky: sky_weights(env, hour),
            clouds: clouds(env, hour),
            cloud_colour: colour(rows::CLOUDS, [0.0; 3]).map(|c| c.powf(2.2)),
            shadow_proj: glam::Mat4::IDENTITY,
            shadow: [0.0; 4],
            points: [[0.0; 4]; 2 * MAX_POINTS],
            point_count: 0,
            brightness: BRIGHTNESS,
            sky_brightness: BRIGHTNESS,
            highlight_limit: HIGHLIGHT_LIMIT,
        }
    }
}

/// Fills `light`'s point lights with the [`MAX_POINTS`] of `lights` nearest
/// `focus` that can reach near it, at `time` seconds (for flicker). Lamps lit
/// only at night fade out as the sun's light grows.
pub fn set_points(light: &mut Light, lights: &[PointLight], focus: Vec3, time: f32) {
    let sun = light.sun.iter().sum::<f32>() / 3.0;
    let daylight = ((sun - 0.15) / 0.5).clamp(0.0, 1.0);
    let mut near: Vec<(f32, usize)> = lights
        .iter()
        .enumerate()
        .map(|(i, l)| (l.position.distance(focus) - l.outer_radius, i))
        .filter(|&(d, _)| d < 60.0)
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    light.point_count = 0;
    for &(_, i) in near.iter().take(MAX_POINTS) {
        let l = &lights[i];
        let mut strength = flicker(l.flicker, l.flicker_speed, time, i as f32);
        if l.night_only {
            strength *= 1.0 - daylight;
        }
        if strength <= 0.0 {
            continue;
        }
        let k = light.point_count as usize;
        light.points[2 * k] = l.position.extend(l.outer_radius).to_array();
        let [r, g, b] = l.colour.map(|c| c * strength);
        light.points[2 * k + 1] = [r, g, b, l.inner_radius];
        light.point_count += 1;
    }
}

/// A light's strength (0..1) at `time`: steady without flicker, else wavering
/// down by up to `amount`, faster for a smaller `speed`. Two sines with a
/// per-light phase stand in for the game's flicker (by eye).
fn flicker(amount: f32, speed: f32, time: f32, phase: f32) -> f32 {
    if amount <= 0.0 {
        return 1.0;
    }
    let rate = std::f32::consts::TAU / speed.max(0.05);
    let wave = 0.5
        + 0.3 * (time * rate + phase * 1.3).sin()
        + 0.2 * (time * rate * 2.7 + phase * 2.1).sin();
    1.0 - amount * 0.6 * wave.clamp(0.0, 1.0)
}

/// How much of the key row lights interiors (by eye).
const INDOOR_KEY: f32 = 0.5;

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
    const BLEND: u32 = fields::SKY_TEXTURE_1_BLEND;
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

/// The cloud layers at `hour`: each layer's texture from the nearer keyframe,
/// its opacity blended between the two.
fn clouds(env: &Environment, hour: f32) -> [(u32, f32); 2] {
    let Some((a, b, t)) = env.between(hour) else {
        return [(0, 0.0); 2];
    };
    let (ka, kb) = (&env.keyframes[a], &env.keyframes[b]);
    let layer = |texture: u32, opacity: u32| {
        let near = if t < 0.5 { ka } else { kb };
        let id = near.u32(texture).unwrap_or(0);
        let o = |k: &fable_data::environment::Keyframe| k.f32(opacity).unwrap_or(0.0);
        (id, (o(ka) * (1.0 - t) + o(kb) * t).clamp(0.0, 1.0))
    };
    [
        layer(fields::CLOUD_TEXTURE_0, fields::CLOUD_OPACITY_0),
        layer(fields::CLOUD_TEXTURE_1, fields::CLOUD_OPACITY_1),
    ]
}

/// A point light placed in a region: a lamp, a fire or a light marker.
#[derive(Debug, Clone, PartialEq)]
pub struct PointLight {
    pub position: Vec3,
    /// RGB 0..1, as stored (gamma space).
    pub colour: [f32; 3],
    pub inner_radius: f32,
    pub outer_radius: f32,
    pub flicker: f32,
    pub flicker_speed: f32,
    /// Lit only at night (dimmed by daylight).
    pub night_only: bool,
}

impl PointLight {
    /// The light of a placed thing: its definition's `CLightDef`, switched by
    /// its `CTCLight` component's `Active` and with the values it overrides.
    /// Lamps whose definition is named unlit or off are skipped: they are
    /// placed active but show no flame (a guess from the names).
    pub fn from_thing(t: &fable_data::tng::Thing, defs: &Defs, origin: Vec3) -> Option<Self> {
        let component = t.component("CTCLight")?;
        let field = |k: &str| {
            component
                .fields
                .iter()
                .find(|f| f.key == k)
                .map(|f| f.value.as_str())
        };
        if field("Active") == Some("FALSE") {
            return None;
        }
        let definition = t.definition()?;
        if definition.contains("_UNLIT") || definition.ends_with("_OFF") {
            return None;
        }
        let def = defs.get(definition)?;
        let record = def
            .component("CLightDef")
            .and_then(|i| defs.defs.get(i))
            .filter(|c| defs.type_name(c) == "CLightDef")?;
        let light = fable_data::light::LightDef::parse(&record.record).ok()?;
        let overridden = field("Overridden") == Some("TRUE");
        let number = |k: &str, default: f32| {
            overridden
                .then(|| field(k)?.parse::<f32>().ok())
                .flatten()
                .unwrap_or(default)
        };
        let colour = overridden
            .then(|| fable_data::light::parse_tng_colour(field("Colour")?))
            .flatten()
            .unwrap_or(light.colour);
        Some(Self {
            position: origin + Vec3::from_array(t.position()?),
            colour: colour.map(|c| f32::from(c) / 255.0),
            inner_radius: number("InnerRadius", light.inner_radius),
            outer_radius: number("OuterRadius", light.outer_radius),
            flicker: number("Flicker", light.flicker),
            flicker_speed: light.flicker_speed,
            night_only: light.daylight_attenuator,
        })
        .filter(|l| l.outer_radius > 0.0 && l.colour.iter().any(|&c| c > 0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_lights_are_used_and_lamps_go_out_by_day() {
        let lamp = |x: f32, night_only| PointLight {
            position: Vec3::new(x, 0.0, 0.0),
            colour: [0.5, 0.25, 0.1],
            inner_radius: 4.0,
            outer_radius: 6.0,
            flicker: 0.0,
            flicker_speed: 0.3,
            night_only,
        };
        let lights: Vec<_> = (0..20).map(|i| lamp(i as f32 * 2.0, false)).collect();
        let mut light = Light::default();
        set_points(&mut light, &lights, Vec3::ZERO, 0.0);
        assert_eq!(light.point_count as usize, MAX_POINTS);
        assert_eq!(light.points[0], [0.0, 0.0, 0.0, 6.0], "nearest first");
        light.sun = [1.0; 3];
        set_points(&mut light, &[lamp(0.0, true)], Vec3::ZERO, 0.0);
        assert_eq!(light.point_count, 0, "a night lamp at noon");
        assert!((flicker(0.5, 0.3, 1.0, 0.0) - 1.0).abs() <= 0.3);
    }

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
