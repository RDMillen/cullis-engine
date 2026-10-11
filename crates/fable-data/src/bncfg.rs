//! `Bones/*.bncfg`: per-bone scales that reshape a skeleton for one kind of
//! character (`hero_young_set.bncfg` makes the Hero a child). See
//! `docs/formats/bncfg.md`.
//!
//! Text, CRLF lines:
//!
//! ```text
//! Creature_type: CREATURE_HERO_CHILD_02;
//! #Start_group_settings
//! clavs: "Bip01 L Clavicle", "Bip01 R Clavicle";
//! #End_group_settings
//! #Start_Bone_data
//! Bip01 L Thigh: 0.6, 0.75, 0.75;
//! #End_bone_data
//! ```

use crate::{Error, Result};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BoneConfig {
    /// The creature definition it was made for (informational).
    pub creature_type: String,
    /// Named groups of bones (`clavs`, `hands`, …); their use is not known.
    pub groups: Vec<(String, Vec<String>)>,
    /// Per bone: scale along the bone (x, its length) and across it (y, z).
    pub bones: Vec<(String, [f32; 3])>,
}

impl BoneConfig {
    pub fn parse(text: &str) -> Result<Self> {
        let mut config = Self::default();
        let mut section = "";
        for (n, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(tag) = line.strip_prefix('#') {
                section = match tag.to_ascii_lowercase().as_str() {
                    "start_group_settings" => "groups",
                    "start_bone_data" => "bones",
                    _ => "",
                };
                continue;
            }
            let err = |reason: &str| Error::Invalid {
                offset: n,
                reason: format!("line {}: {reason}: {line:?}", n + 1),
            };
            let (key, value) = line.split_once(':').ok_or_else(|| err("no ':'"))?;
            let value = value.trim().trim_end_matches(';').trim();
            match section {
                "groups" => config.groups.push((
                    key.trim().to_owned(),
                    value
                        .split(',')
                        .map(|b| b.trim().trim_matches('"').to_owned())
                        .collect(),
                )),
                "bones" => {
                    let v: Vec<f32> = value
                        .split(',')
                        .map(|x| x.trim().parse::<f32>())
                        .collect::<std::result::Result<_, _>>()
                        .map_err(|_| err("bad scale"))?;
                    let scale: [f32; 3] = v.try_into().map_err(|_| err("not 3 scales"))?;
                    config.bones.push((key.trim().to_owned(), scale));
                }
                _ if key.trim().eq_ignore_ascii_case("Creature_type") => {
                    config.creature_type = value.to_owned();
                }
                _ => return Err(err("outside a section")),
            }
        }
        Ok(config)
    }

    /// The scale of bone `name` (case-insensitive), if the file sets one.
    pub fn scale(&self, name: &str) -> Option<[f32; 3]> {
        self.bones
            .iter()
            .find(|(b, _)| b.eq_ignore_ascii_case(name))
            .map(|(_, s)| *s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_groups_and_bones() {
        let text = "Creature_type: CREATURE_YOUNG_SISTER;\r\n#Start_group_settings\r\nclavs: \"Bip01 R Clavicle\", \"Bip01 L Clavicle\";\r\n#End_group_settings\r\n#Start_Bone_data\r\nBip01 L Thigh: 0.7, 1.0, 1.0;\r\nSub_movement_dummy: 1.0, 1.0, 0.75;\r\n#End_bone_data\r\n";
        let c = BoneConfig::parse(text).unwrap();
        assert_eq!(c.creature_type, "CREATURE_YOUNG_SISTER");
        assert_eq!(c.groups[0].1, ["Bip01 R Clavicle", "Bip01 L Clavicle"]);
        assert_eq!(c.scale("bip01 l thigh"), Some([0.7, 1.0, 1.0]));
        assert_eq!(c.scale("Sub_movement_dummy"), Some([1.0, 1.0, 0.75]));
        assert_eq!(c.scale("Bip01 Head"), None);
    }

    #[test]
    fn rejects_short_scales() {
        assert!(BoneConfig::parse("#Start_Bone_data\nBip01: 1.0, 2.0;\n").is_err());
    }
}
