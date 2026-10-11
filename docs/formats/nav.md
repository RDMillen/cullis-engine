# Navigation

Each Lev section ([lev.md](lev.md), "After the grid") holds a navigation quadtree for one map state. Parser: `fable_data::nav::Navigation`. All 233 sections in all 398 maps parse exactly: 335 layers, 252,517 leaves (`tests/real_data.rs`).

Integers are little-endian; records are unaligned.

## Section

```
u32 end, u32 8, f32 width, f32 height      section header (lev.md)
u32 unknown                                4 in the Arena, 10 in StartOakValeWest
u32 points
points x { f32 x, f32 y, u32 end }         outline points; end = 1 on the last point of a run
u32 layers                                 0 to 5
u32 node count                             equals the largest node id in most sections
layers x (width/32 x height/32 roots, row by row)
u32 trailer                                probably a checksum
```

Map sizes are multiples of 32 in every map.

## Roots and nodes

A root is either `00 01 01` (3 bytes: an empty 32x32 block, no navigation) or a quadtree written depth-first:

| Field | Type | Notes |
| --- | --- | --- |
| flags | 4 bytes | `[0]` = 1: a leaf with extra pairs; `[1]` = 1: a root; `[3]` = 1: a leaf |
| level | u8 | 0 for a root; a node's size is 32 / 2^level (down to 0.5 at level 6) |
| sub | u8 | 1 on some roots of later layers, otherwise 0 |
| centre | 2 x f32 | map coordinates |
| id | u32 | |

Interior nodes then have four `u32` child ids (0 = no child in that quadrant). Leaves have:

| Field | Type | Notes |
| --- | --- | --- |
| value | u32 | 1–9 in StartOakValeWest; meaning unknown |
| kind | u8 | 0 on paths, 128 on other walkable ground, 64 on a second layer (StartOakValeWest) |
| neighbour count, neighbours | u32, n x u32 | adjacent leaf ids |
| extras | u32 count, count x (u32, u32) | only when flags[0] = 1; the first value looks like an id |

## What it shows

Drawn over the terrain (viewer `--nav`), StartOakValeWest's leaves cover exactly the village paths (kind 0) and the walkable ground around them (kind 128), with holes at houses, the well and fenced areas; hillsides, cliffs and the sea have no leaves. Its second layer covers the bridge deck across the ravine and raised parts of houses, so layers are walking levels. The navigation data holds no heights.

The viewer gives cells their heights from the terrain and the placed meshes' upward-facing triangles (`walk::assign_surfaces`), growing out from known heights one 0.6-unit step at a time and preferring the highest surface within a step. In StartOakVale this puts the whole second-layer bridge deck (342 cells) on the planks at 21.81, reached up a ramp on the first layer from the plateau at 21.0, and the Hero can walk across it. Three other second-layer patches (about 880 cells, under or inside buildings, e.g. around (3544–3563, 823–836)) have no upward surface within a step of the first layer, so what they are is not known; they stay unwalkable.

Not decoded: leaf `value`, the extras, the outline points and the `unknown` count.
