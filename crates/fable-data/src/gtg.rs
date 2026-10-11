//! `Levels/FinalAlbion.gtg`: world-level things (region entrances, player starts,
//! global markers) for each map. See `docs/formats/gtg.md`.
//!
//! The file is a series of `NEWMAP <wld map number>` ... `ENDMAP` blocks, each
//! holding Tng text ([`crate::tng`]) with positions relative to that map.

use crate::tng::{Thing, Tng};
use crate::{Error, Result};

#[derive(Debug, Clone, Default)]
pub struct Gtg {
    pub maps: Vec<GtgMap>,
}

#[derive(Debug, Clone)]
pub struct GtgMap {
    /// The map's `NewMap` number in the wld.
    pub number: u32,
    pub tng: Tng,
}

/// Mask of the map-local part of a thing UID.
const LOCAL_UID_MASK: u64 = 0xff_ffff_ffff;

/// A region exit's `EntranceConnectedToUID` names its entrance as the destination
/// map's `MapUID` shifted left 40 bits, combined with the low 40 bits of the
/// entrance thing's UID (whose own top bits are `0xfffffe` in every thing). Returns
/// (map UID, local UID).
pub fn split_connected_uid(uid: u64) -> (u32, u64) {
    ((uid >> 40) as u32, uid & LOCAL_UID_MASK)
}

impl Gtg {
    pub fn parse(text: &str) -> Result<Self> {
        let mut maps = Vec::new();
        let mut current: Option<(u32, String)> = None;
        for (i, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if let Some(n) = trimmed.strip_prefix("NEWMAP") {
                let number = n.trim().parse().map_err(|_| Error::Invalid {
                    offset: i,
                    reason: format!("line {}: bad NEWMAP {n:?}", i + 1),
                })?;
                current = Some((number, String::new()));
            } else if trimmed == "ENDMAP" {
                let (number, body) = current.take().ok_or_else(|| Error::Invalid {
                    offset: i,
                    reason: format!("line {}: ENDMAP without NEWMAP", i + 1),
                })?;
                maps.push(GtgMap {
                    number,
                    tng: Tng::parse(&body)?,
                });
            } else if let Some((_, body)) = &mut current {
                body.push_str(line);
                body.push('\n');
            }
        }
        if current.is_some() {
            return Err(Error::Invalid {
                offset: text.len(),
                reason: "NEWMAP without ENDMAP".into(),
            });
        }
        Ok(Self { maps })
    }

    pub fn map(&self, number: u32) -> Option<&GtgMap> {
        self.maps.iter().find(|m| m.number == number)
    }
}

impl GtgMap {
    /// The thing whose UID's map-local part is `local`.
    pub fn thing(&self, local: u64) -> Option<&Thing> {
        self.tng
            .things
            .iter()
            .find(|t| t.uid().is_some_and(|u| u & LOCAL_UID_MASK == local))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "NEWMAP 1\nVersion 2;\n\nXXXSectionStart NULL;\n\nNewThing Thing;\nUID 18446741874686296173;\nDefinitionType \"REGION_ENTRANCE_POINT\";\nStartCTCPhysicsStandard;\nPositionX 1.5;\nPositionY 2.5;\nPositionZ 3.5;\nEndCTCPhysicsStandard;\nEndThing;\n\nXXXSectionEnd;\nENDMAP\nNEWMAP 7\nVersion 2;\n\nENDMAP\n";

    #[test]
    fn parses_maps_and_resolves_connections() {
        let gtg = Gtg::parse(TEXT).unwrap();
        assert_eq!(gtg.maps.len(), 2);
        assert!(gtg.map(7).unwrap().tng.things.is_empty());
        // LookoutPoint's MapUID (162441) and the entrance's local UID 0x6d.
        let (map_uid, local) = split_connected_uid(0x027a_8900_0000_006d);
        assert_eq!((map_uid, local), (162441, 0x6d));
        let entrance = gtg.map(1).unwrap().thing(local).unwrap();
        assert_eq!(entrance.definition(), Some("REGION_ENTRANCE_POINT"));
        assert_eq!(entrance.position(), Some([1.5, 2.5, 3.5]));
    }

    #[test]
    fn rejects_an_unclosed_map() {
        assert!(Gtg::parse("NEWMAP 1\nVersion 2;\n").is_err());
    }
}
