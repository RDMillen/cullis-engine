//! `Levels/FinalAlbion.wld`: the world's maps and regions, as `Key value;` text.
//! See `docs/formats/wld.md`.

use std::path::Path;

use crate::text::statements;
use crate::Result;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Wld {
    /// Quest names listed between `START_INITIAL_QUESTS;` and `END_INITIAL_QUESTS;`.
    pub initial_quests: Vec<String>,
    pub map_uid_count: u32,
    pub thing_manager_uid_count: u32,
    pub maps: Vec<WldMap>,
    pub regions: Vec<WldRegion>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WldMap {
    pub number: u32,
    pub x: i32,
    pub y: i32,
    /// Lev path relative to `Data\Levels\`, for example `FinalAlbion\LookoutPoint.lev`.
    pub level_name: String,
    pub level_script_name: String,
    pub uid: u32,
    pub is_sea: bool,
    pub loaded_on_player_proximity: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WldRegion {
    pub number: u32,
    pub name: String,
    /// Text key, for example `TXT_REGION_LOOKOUT_POINT`.
    pub display_name: String,
    /// Def name, for example `REGION_LOOKOUT_POINT`. Empty for filler regions.
    pub region_def: String,
    pub appear_on_world_map: bool,
    pub mini_map_graphic: Option<String>,
    pub mini_map_scale: f32,
    pub mini_map_offset: [f32; 2],
    pub world_map_offset: [f32; 2],
    pub name_graphic_offset: [f32; 2],
    pub exit_text_offsets: Vec<ExitTextOffset>,
    /// Maps that belong to this region.
    pub contains_maps: Vec<String>,
    /// Maps outside the region that are visible from it.
    pub sees_maps: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExitTextOffset {
    pub region: String,
    pub offset: [f32; 2],
}

impl Wld {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::parse(&crate::reader::latin1(&bytes))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let mut wld = Wld::default();
        let mut stmts = statements(text)?.into_iter();
        while let Some(s) = stmts.next() {
            match s.key {
                "START_INITIAL_QUESTS" => loop {
                    let q = stmts
                        .next()
                        .ok_or_else(|| s.err("unterminated START_INITIAL_QUESTS"))?;
                    if q.key == "END_INITIAL_QUESTS" {
                        break;
                    }
                    wld.initial_quests.push(q.key.to_owned());
                },
                "MapUIDCount" => wld.map_uid_count = s.number()?,
                "ThingManagerUIDCount" => wld.thing_manager_uid_count = s.number()?,
                "NewMap" => {
                    let mut map = WldMap {
                        number: s.number()?,
                        ..Default::default()
                    };
                    loop {
                        let f = stmts.next().ok_or_else(|| s.err("unterminated NewMap"))?;
                        match f.key {
                            "EndMap" => break,
                            "MapX" => map.x = f.number()?,
                            "MapY" => map.y = f.number()?,
                            "LevelName" => map.level_name = f.string()?,
                            "LevelScriptName" => map.level_script_name = f.string()?,
                            "MapUID" => map.uid = f.number()?,
                            "IsSea" => map.is_sea = f.bool()?,
                            "LoadedOnPlayerProximity" => {
                                map.loaded_on_player_proximity = f.bool()?
                            }
                            k => return Err(f.err(format!("unknown map key {k:?}"))),
                        }
                    }
                    wld.maps.push(map);
                }
                "NewRegion" => {
                    let mut region = WldRegion {
                        number: s.number()?,
                        ..Default::default()
                    };
                    loop {
                        let f = stmts
                            .next()
                            .ok_or_else(|| s.err("unterminated NewRegion"))?;
                        match f.key {
                            "EndRegion" => break,
                            "RegionName" => region.name = f.string()?,
                            "NewDisplayName" => region.display_name = f.string()?,
                            "RegionDef" => region.region_def = f.string()?,
                            "AppearOnWorldMap" => region.appear_on_world_map = true,
                            "MiniMapGraphic" => region.mini_map_graphic = Some(f.value.to_owned()),
                            "MiniMapScale" => region.mini_map_scale = f.number()?,
                            "MiniMapOffsetX" => region.mini_map_offset[0] = f.number()?,
                            "MiniMapOffsetY" => region.mini_map_offset[1] = f.number()?,
                            "WorldMapOffsetX" => region.world_map_offset[0] = f.number()?,
                            "WorldMapOffsetY" => region.world_map_offset[1] = f.number()?,
                            "NameGraphicOffsetX" => region.name_graphic_offset[0] = f.number()?,
                            "NameGraphicOffsetY" => region.name_graphic_offset[1] = f.number()?,
                            "MiniMapRegionExitTextOffsetX" | "MiniMapRegionExitTextOffsetY" => {
                                let target = f.arg.ok_or_else(|| f.err("missing [region]"))?;
                                let axis = usize::from(f.key.ends_with('Y'));
                                let i = match region
                                    .exit_text_offsets
                                    .iter()
                                    .position(|e| e.region == target)
                                {
                                    Some(i) => i,
                                    None => {
                                        region.exit_text_offsets.push(ExitTextOffset {
                                            region: target.to_owned(),
                                            offset: [0.0; 2],
                                        });
                                        region.exit_text_offsets.len() - 1
                                    }
                                };
                                region.exit_text_offsets[i].offset[axis] = f.number()?;
                            }
                            "ContainsMap" => region.contains_maps.push(f.string()?),
                            "SeesMap" => region.sees_maps.push(f.string()?),
                            k => return Err(f.err(format!("unknown region key {k:?}"))),
                        }
                    }
                    wld.regions.push(region);
                }
                k => return Err(s.err(format!("unknown top-level key {k:?}"))),
            }
        }
        Ok(wld)
    }

    pub fn map(&self, level_name: &str) -> Option<&WldMap> {
        self.maps
            .iter()
            .find(|m| m.level_name.eq_ignore_ascii_case(level_name))
    }

    pub fn region(&self, name: &str) -> Option<&WldRegion> {
        self.regions
            .iter()
            .find(|r| r.name.eq_ignore_ascii_case(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    const SAMPLE: &str = "START_INITIAL_QUESTS;\r\nQ_One;\r\nEND_INITIAL_QUESTS;\r\n\r\n\
        MapUIDCount 3;\r\nThingManagerUIDCount 1;\r\n\r\n\
        NewMap 1;\r\nMapX -32;\r\nMapY 64;\r\nLevelName \"Test\\A.lev\";\r\nLevelScriptName \"A\";\r\n\
        MapUID 7;\r\nIsSea FALSE;\r\nLoadedOnPlayerProximity TRUE;\r\nEndMap;\r\n\r\n\
        NewRegion 1;\r\nRegionName \"A\";\r\nNewDisplayName \"TXT_A\";\r\nRegionDef \"REGION_A\";\r\n\
        AppearOnWorldMap;\r\nMiniMapGraphic MINIMAP_A;\r\nMiniMapScale 1.5;\r\n\
        MiniMapRegionExitTextOffsetX[B] 2.0;\r\nMiniMapRegionExitTextOffsetY[B] -3.0;\r\n\
        ContainsMap \"Test\\A.lev\";\r\nSeesMap \"Test\\B.lev\";\r\nEndRegion;\r\n";

    #[test]
    fn parses_sample() {
        let wld = Wld::parse(SAMPLE).unwrap();
        assert_eq!(wld.initial_quests, ["Q_One"]);
        assert_eq!(wld.map_uid_count, 3);
        let map = wld.map("test\\a.lev").unwrap();
        assert_eq!(
            (
                map.x,
                map.y,
                map.uid,
                map.is_sea,
                map.loaded_on_player_proximity
            ),
            (-32, 64, 7, false, true)
        );
        let region = wld.region("A").unwrap();
        assert!(region.appear_on_world_map);
        assert_eq!(region.mini_map_graphic.as_deref(), Some("MINIMAP_A"));
        assert_eq!(
            region.exit_text_offsets,
            [ExitTextOffset {
                region: "B".into(),
                offset: [2.0, -3.0]
            }]
        );
        assert_eq!(region.contains_maps, ["Test\\A.lev"]);
        assert_eq!(region.sees_maps, ["Test\\B.lev"]);
    }

    #[test]
    fn rejects_unknown_key() {
        let err = Wld::parse("NewMap 1;\nMapZ 3;\nEndMap;\n").unwrap_err();
        assert!(matches!(err, Error::Text { line: 2, .. }), "{err}");
    }

    #[test]
    fn brackets_inside_a_key_are_not_an_argument() {
        let text = "NewMap 1;\nKeyCameras[0].Position 3;\nEndMap;\n";
        let err = Wld::parse(text).unwrap_err();
        assert!(err.to_string().contains("KeyCameras[0].Position"), "{err}");
    }

    #[test]
    fn rejects_missing_semicolon() {
        assert!(matches!(
            Wld::parse("MapUIDCount 3\n"),
            Err(Error::Text { line: 1, .. })
        ));
    }
}
