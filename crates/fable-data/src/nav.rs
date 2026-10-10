//! Navigation quadtrees in Lev sections ([`crate::lev::LevSection`]).
//! See `docs/formats/nav.md`.
//!
//! Each section holds an outline point list, then one or more layers. A layer is a
//! grid of 32x32-unit roots, each an empty marker or a quadtree of nodes down to
//! 0.5 units. Leaves are the walkable cells: in StartOakValeWest they cover the
//! paths and the ground around them, with holes at buildings; a second layer
//! covers the bridge deck and raised floors.

use crate::reader::Reader;
use crate::{Error, Result};

/// Side of a root cell in world units; a node at level `l` is `ROOT_SIZE / 2^l`.
pub const ROOT_SIZE: f32 = 32.0;
const SECTION_HEADER_LEN: usize = 16;

#[derive(Debug, Clone, PartialEq)]
pub struct Navigation {
    /// Unknown count before the outline (4 in the Arena, 10 in StartOakValeWest).
    pub unknown: u32,
    /// Points of outlines; `end` marks the last point of a run.
    pub outline: Vec<OutlinePoint>,
    /// Matches the largest node id in most sections.
    pub node_count: u32,
    pub layers: Vec<Layer>,
    /// Last 4 bytes of the section; probably a checksum.
    pub trailer: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutlinePoint {
    pub x: f32,
    pub y: f32,
    pub end: bool,
}

/// One walking level: `columns * rows` roots, row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub columns: u32,
    pub rows: u32,
    /// `None` for an empty root (no navigation in that 32x32 block).
    pub roots: Vec<Option<Vec<Node>>>,
}

/// A quadtree node, depth-first in file order.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Four flag bytes: `[0]` = 1 when a leaf has extra pairs, `[1]` = 1 for a root,
    /// `[3]` = 1 for a leaf.
    pub flags: [u8; 4],
    /// 0 for a root; size = [`ROOT_SIZE`] / 2^level.
    pub level: u8,
    /// Second level byte: 1 for some roots of later layers; otherwise 0.
    pub sub: u8,
    /// Centre in map coordinates.
    pub centre: [f32; 2],
    pub id: u32,
    pub kind: NodeKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    /// Child node ids; 0 means no child in that quadrant.
    Interior {
        children: [u32; 4],
    },
    Leaf(Leaf),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Leaf {
    /// 1–9 in StartOakValeWest; meaning unknown.
    pub value: u32,
    /// 0 on paths, 128 on other walkable ground, 64 on a second layer (StartOakValeWest).
    pub kind: u8,
    /// Ids of adjacent leaves.
    pub neighbours: Vec<u32>,
    /// Extra (u32, u32) pairs when `flags[0]` is 1; the first value looks like an id.
    pub extras: Vec<(u32, u32)>,
}

impl Node {
    pub fn size(&self) -> f32 {
        ROOT_SIZE / f32::from(1u16 << self.level.min(15))
    }

    pub fn is_leaf(&self) -> bool {
        matches!(self.kind, NodeKind::Leaf(_))
    }
}

impl Navigation {
    /// Parses a section's data (as in [`crate::lev::LevSection::data`]) for a map of
    /// `width` x `height` units.
    pub fn parse(section: &[u8], width: u32, height: u32) -> Result<Self> {
        if !width.is_multiple_of(32) || !height.is_multiple_of(32) {
            return Err(Error::Invalid {
                offset: 0,
                reason: format!("map size {width}x{height} is not a multiple of 32"),
            });
        }
        let body = section
            .get(SECTION_HEADER_LEN..section.len().saturating_sub(4))
            .ok_or(Error::UnexpectedEof {
                offset: 0,
                wanted: SECTION_HEADER_LEN + 4,
            })?;
        let trailer = u32::from_le_bytes(section[section.len() - 4..].try_into().unwrap());
        let mut r = Reader::new(body);
        let unknown = r.u32()?;
        let points = r.u32()?;
        let mut outline = Vec::with_capacity(points as usize);
        for _ in 0..points {
            outline.push(OutlinePoint {
                x: f32::from_bits(r.u32()?),
                y: f32::from_bits(r.u32()?),
                end: r.u32()? != 0,
            });
        }
        let layer_count = r.u32()?;
        let node_count = r.u32()?;
        let (columns, rows) = (width / 32, height / 32);
        let mut layers = Vec::with_capacity(layer_count as usize);
        for _ in 0..layer_count {
            let mut roots = Vec::with_capacity((columns * rows) as usize);
            for _ in 0..columns * rows {
                roots.push(parse_root(body, &mut r, width as f32, height as f32)?);
            }
            layers.push(Layer {
                columns,
                rows,
                roots,
            });
        }
        if r.pos() != body.len() {
            return Err(Error::Invalid {
                offset: SECTION_HEADER_LEN + r.pos(),
                reason: format!("{} bytes left before the trailer", body.len() - r.pos()),
            });
        }
        Ok(Self {
            unknown,
            outline,
            node_count,
            layers,
            trailer,
        })
    }

    /// Every leaf in every layer, with its layer index.
    pub fn leaves(&self) -> impl Iterator<Item = (usize, &Node, &Leaf)> {
        self.layers.iter().enumerate().flat_map(|(i, layer)| {
            layer
                .roots
                .iter()
                .flatten()
                .flatten()
                .filter_map(move |n| match &n.kind {
                    NodeKind::Leaf(leaf) => Some((i, n, leaf)),
                    NodeKind::Interior { .. } => None,
                })
        })
    }
}

const EMPTY_ROOT: [u8; 3] = [0, 1, 1];

/// One root: an empty marker, or a node and (depth-first) all its descendants.
fn parse_root(body: &[u8], r: &mut Reader, width: f32, height: f32) -> Result<Option<Vec<Node>>> {
    if body.get(r.pos()..r.pos() + 3) == Some(&EMPTY_ROOT) {
        r.bytes(3)?;
        return Ok(None);
    }
    let mut nodes = Vec::new();
    let mut pending = 1;
    while pending > 0 {
        pending -= 1;
        let at = r.pos();
        let flags: [u8; 4] = r.array()?;
        let [level, sub]: [u8; 2] = r.array()?;
        let centre = [f32::from_bits(r.u32()?), f32::from_bits(r.u32()?)];
        if level > 8
            || !(centre[0] > 0.0 && centre[0] <= width && centre[1] > 0.0 && centre[1] <= height)
        {
            return Err(Error::Invalid {
                offset: SECTION_HEADER_LEN + at,
                reason: format!("navigation node level {level} at {centre:?}"),
            });
        }
        let id = r.u32()?;
        let kind = if flags[3] == 1 {
            let value = r.u32()?;
            let [kind]: [u8; 1] = r.array()?;
            let n = r.u32()?;
            let neighbours = (0..n).map(|_| r.u32()).collect::<Result<_>>()?;
            let extras = if flags[0] == 1 {
                let m = r.u32()?;
                (0..m)
                    .map(|_| Ok((r.u32()?, r.u32()?)))
                    .collect::<Result<_>>()?
            } else {
                Vec::new()
            };
            NodeKind::Leaf(Leaf {
                value,
                kind,
                neighbours,
                extras,
            })
        } else {
            let children = [r.u32()?, r.u32()?, r.u32()?, r.u32()?];
            pending += children.iter().filter(|&&c| c != 0).count();
            NodeKind::Interior { children }
        };
        nodes.push(Node {
            flags,
            level,
            sub,
            centre,
            id,
            kind,
        });
    }
    Ok(Some(nodes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(out: &mut Vec<u8>, words: &[u32]) {
        for w in words {
            out.extend_from_slice(&w.to_le_bytes());
        }
    }

    /// A 32x32 map: one root with one leaf child at (8, 8), plus an empty second layer.
    fn section() -> Vec<u8> {
        let mut s = vec![0; SECTION_HEADER_LEN];
        put(&mut s, &[4, 1]);
        put(&mut s, &[5.0f32.to_bits(), 6.0f32.to_bits(), 1]);
        put(&mut s, &[2, 2]);
        // Root: interior, level 0, centre (16, 16), id 2, one child (id 1).
        s.extend_from_slice(&[0, 1, 0, 0, 0, 0]);
        put(
            &mut s,
            &[16.0f32.to_bits(), 16.0f32.to_bits(), 2, 0, 0, 1, 0],
        );
        // Leaf: level 1, centre (8, 8), id 1, value 3, kind 128, neighbours [7], one extra.
        s.extend_from_slice(&[1, 0, 0, 1, 1, 0]);
        put(&mut s, &[8.0f32.to_bits(), 8.0f32.to_bits(), 1, 3]);
        s.push(128);
        put(&mut s, &[1, 7, 1, 611, 0]);
        // Second layer: an empty root.
        s.extend_from_slice(&EMPTY_ROOT);
        put(&mut s, &[0xdead_beef]);
        s
    }

    #[test]
    fn parses_layers_roots_and_leaves() {
        let nav = Navigation::parse(&section(), 32, 32).unwrap();
        assert_eq!(
            (nav.unknown, nav.node_count, nav.trailer),
            (4, 2, 0xdead_beef)
        );
        assert_eq!(
            nav.outline,
            [OutlinePoint {
                x: 5.0,
                y: 6.0,
                end: true
            }]
        );
        assert_eq!(nav.layers.len(), 2);
        assert_eq!(nav.layers[1].roots, [None]);
        let leaves: Vec<_> = nav.leaves().collect();
        assert_eq!(leaves.len(), 1);
        let (layer, node, leaf) = leaves[0];
        assert_eq!(
            (layer, node.id, node.size(), node.centre),
            (0, 1, 16.0, [8.0, 8.0])
        );
        assert_eq!(
            (
                leaf.value,
                leaf.kind,
                leaf.neighbours.clone(),
                leaf.extras.clone()
            ),
            (3, 128, vec![7], vec![(611, 0)])
        );
    }

    #[test]
    fn rejects_leftover_bytes() {
        let mut s = section();
        let trailer = s.split_off(s.len() - 4);
        s.push(0);
        s.extend(trailer);
        assert!(Navigation::parse(&s, 32, 32).is_err());
    }
}
