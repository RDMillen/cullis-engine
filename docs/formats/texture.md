# Textures

Entries of the `GBANK_*` banks in `graphics/pc/textures.big` and `graphics/pc/frontend.big` ([big.md](big.md)). Parser: `fable_data::texture`; DXT decoding: `fable_data::dxt`. All 6,717 textures in the reference install unpack with no bytes left over, except one in an unsupported format (`tests/real_data.rs`).

## Info block (34 bytes)

| Offset | Type | Field | Notes |
| --- | --- | --- | --- |
| 0 | u16 | width | |
| 2 | u16 | height | |
| 4 | u16 | depth | 0, or the slice count of a volume texture (8 for `WEATHER_RAIN_PC`) |
| 6 | u16 | frame width | Equal to width except for sprite sheets and frame sequences |
| 8 | u16 | frame height | |
| 10 | u16 | frame count | |
| 12 | u8 | format | See below |
| 16 | u8 | alpha flag | 0 opaque, 1 alpha, 3 seen on the DXT5 sky |
| 17 | u8 | mip count | Levels stored per image |
| 20 | u32 | top level length | Bytes of the top level as stored |
| 24 | u32 | compressed length | Bytes the compressed top level takes; 0 when nothing is compressed |
| 29 | u8 | bits per pixel | 4, 8, 16 or 32 |

| Format | Bits | Count | Meaning |
| --- | --- | --- | --- |
| `0x1f` | 4 | 3,703 | DXT1 |
| `0x20` | 8 | 2,574 | DXT3 (confirmed by eye: smooth alpha edges on `SEAGULL_WING_32`) |
| `0x23` | 8 | 4 | DXT5 (the atmospheric skies) |
| `0x01` | 32 | 8 | Uncompressed, bytes B, G, R, A |
| `0x18` | 16 | 1 | Not supported yet |

## Layout

The bank entry type decides the layout:

- **Type 2 (bump maps)**: one image, as type 0; see [Bump maps](#bump-maps).
- **Type 1 (frame sequences, e.g. `GRAPHIC_ANIM_CAMPFIRE`)**: `frame count` images of `frame width` x `frame height`, one after another, each with its own mip chain, all uncompressed.
- **Every other type**: one image of `width` x `height`. Sprite sheets (type 5, e.g. `SPRITE_DUST_01`) keep their frames as tiles inside it; a frame count on other types is metadata only.

Each image is a mip chain, largest level first:

- Levels are stored padded to power-of-two sizes, with the image in the top-left corner (a 20x20 icon is stored as 32x32).
- A volume texture's level holds all its slices one after another.
- Levels of at least 4x4 take whole 4x4 blocks. Smaller levels of a block format take their bits-per-pixel size (a 2x2 DXT5 level is 4 bytes, a 1x1 is 1 byte).

When `compressed length` is non-zero, the first image's top level is stored compressed:

```
u16 stream_len           or 0xffff followed by u32 stream_len
LZO1X stream             stream_len bytes, decompressing to (top level length - 3) bytes
3 bytes                  the last 3 bytes of the top level, raw
```

`compressed length` = header + `stream_len` + 3. A `stream_len` of 0 means the level did not compress and follows raw. The rest of the chain follows uncompressed.

## Exporting

`cargo run --release -p fable-data --example texture_png -- ARCHIVE NAME_PART OUT.png [LEVEL] [IMAGE]` writes one level as PNG.

## Interface sprites

The HUD and menus are textures too. `graphics/pc/textures.big` holds `GBANK_MAIN_PC` (6,290 entries) and `GBANK_GUI_PC` (34 entries, the level editor's icons, `EDITORGUI_*`); `graphics/pc/frontend.big` holds `GBANK_FRONT_END_PC` (394 entries: the menus' text boxes and slot bars, region minimaps). From about entry 4485 of `GBANK_MAIN_PC`, entries have names rather than source paths: interface sprites, Dxt3 with alpha.

| Names | What |
| --- | --- |
| `HUD_HEALTH_BAR_*` | the heart (`_HEART`, `_HEART_GOOD`, `_HEART_EVIL`) in its ring (`_RING`, 64×64, visible in the middle 38), the bar's frame (`_MIDDLE`, `_END`, `_END_ROUND`, `_DIVIDE`, 8×16) and fills (`_HEALTH_SEGMENT`, `_HEALTH_END`, visible in their top 11 rows; `_STAMINA*`) |
| `HUD_STATBOX_*` | a rounded box (`_LEFT_END_CURVE`, `_MIDDLE`, `_END`, `_RIGHT_END_CURVE`, 32 high, visible in 26) and morality faces (`_MORALITY_GOOD/_NEUTRAL/_EVIL`) |
| `HUD_TEXTBOX_*` | a nine-piece box: corners `_TL _TR _BL _BR` (32×32), edges `_TM _BM` (8×32) and `_ML _MR` (32×8), fill `_BACK` (4×4): black at alpha 136 with soft rounded edges |
| `GUI_DIGIT_0`…`_9` | outlined white digits, 32×32 |
| `HUD_ICON_*` | 193 icons: actions (`BATTLE` crossed swords, `BATTLE_UNARMED` fist, `SPEAK`, `INTERACT`), spells, items (`MONEY_BAG`, `CHOCS`, `TEDDY`, …), experience (`EXP_*`), `RENOWN` |
| `HUD_PC_*` | the PC's rings (`MOUSE_RING`, `WEAPON_RINGS`, `ITEM_RINGS_*`, `EXPRESSION_RINGS`), mouse buttons (`MOUSE_LMB`, `MOUSE_RMB`), target brackets, Guild seal |
| `HUD_ORB_*`, `HUD_EXPR_*`, `HUD_DPAD_*`, `HUD_ABXY_*`, `HUD_COMBAT_*`, `HUD_MODES_*`, `HUD_CROSSHAIR_*` | quest orbs, expressions, the Xbox controller's rings and buttons, combat multiplier, modes |
| `PC_UI_*_FRAME_n_SPRITE`, `UI_*` | the PC menus' frames (quest start, finished, failed, death, experience, options, map), each a picture cut into nine tiles (three rows of 256, the last 128 high; the options frame fills 672 by 598 with a 36-high title bar), and backdrops (`UI_OPTIONS_BACKDROP_SPRITE_1`-`6`: a 640 by 512 picture in two rows of 256, 256 and 128 wide) |

### Minimaps

Each region has a map, `MINIMAP_<REGION>` in capitals (`MINIMAP_STARTOAKVALE`, `MINIMAP_HEROGUILDCOMPLEXINSIDE`; 256×256, Dxt3): a circular parchment with a soft edge, north (+y) up. It covers the square of the larger side of the maps the region contains (not those it only sees), centred on them: 256 units for StartOakVale (maps 224×256) and HeroGuildComplexInside, 128 for BarrowFields, so the scale differs between regions. Checked by laying each map over a top-down render (`engine_world::region::MapArea`). `frontend.big` has the same maps as `MINIMAP_<REGION>_FRONT_END`.

`MINIMAP_BORDER` (256×128) holds the ring in its top-left 135×113 texels: two small circles at the top (in the HUD, the combat multiplier and the time of day) and sides curving down, open at the bottom. `MINIMAP_ICON_HERO` (64×64) is the Hero's arrow, drawn in texels 24–38 by 14–35. `MINIMAP_ICON_*` (16×16) mark houses, teleports, entrances, enemies and people. `PLAYER_GUI_DEFAULT` lists the marker kinds' textures in order (field `0x9e982d66`, a count of 19, then texture ids): hero, quest orbs core, feat and optional (`HUD_ORB_QUEST_*`), person of interest (twice), teleport, teleport inactive, house, house for sale, demon door, entrance, entrance core, entrance optional, wife, love interest, enemy, henchman, blue; the same definition holds region map names (`MINIMAP_WITCHWOOD4`) with their texture ids. Which things get which marker is not in the data; `GUI_MAP_*` are the world map's. `HUD_CLOCK_FACE` (32×32: night on its left half, day on its right), `HUD_CLOCK_HAND` (a pointer about its centre, pointing right) and `HUD_CLOCK_ICON` (an hourglass) make the time of day.

`engine_ui::Sprites` loads the HUD's by name; `engine_ui::hud` draws the childhood HUD from them (health bar top left, gold box when gold changes, action rings bottom right) and `engine_ui::draw` puts subtitles and notices in the text box.

## Bump maps

Bank entry type 2. In `GBANK_MAIN_PC` there are 372 (357 DXT1, 15 DXT3; `tests/real_data.rs`): mesh materials' second textures (`HERO_CENTRE_INT_WALLS_01_24_B.TGA][BUMP]`, the second texture of the Guild's `WALLS_01`; see [mesh.md](mesh.md)), and others with `BUMP` in the name: about 60 ground surfaces (`UNASSIGNED_COBBLES_SQUARE_01_BUMP`, `UNASSIGNED_PATH_MOSSY_BUMP`, `LANDSCAPE_PAVING_BUMP`, probably the terrain themes'), water (`WATER_BUMP_BIG`, `_SMALL`), and the Hero's skin, scars, muscle, Will and evil looks (`HERO_BUMP_SKIN_FACE`, `COMP_HERO_BUMP_*`). Which ground theme or Hero look uses which is not decoded.

They decode as any other texture and look yellow-green. Red and green are signed values centred on 128 (every map's mean is within 5 of 128): the surface's slope along u (to the right) and v (down the rows), -1 to 1 (`texture::bump_slope`). Blue is not part of it: its mean is 6 to 39 and it is larger where the slope is (on the Guild wall, 6 where flat to 33 at the steepest), as DXT1 rounding would leave when the encoder aimed for 0; nothing else seen fits it.

How this was read, on the Guild's wall (`HERO_CENTRE_INT_WALLS_01_24`) and floor (`HC_INT_FLOOR`):

- Red follows how the colour texture's brightness changes to the right (correlation 0.32 and 0.36), green how it changes down the rows (0.56 and 0.63): grooves and grout are darker, so brighter parts sit higher and both are slopes of that height, rising to the right and downwards.
- Together they behave as slopes of one surface: after a small blur, how red changes down the rows agrees with how green changes to the right (0.37 and 0.54; a pair of slopes of one height has the two equal), more than the other pairing does.

Whether they are slopes or the tilted normal's x and y (the same up to the sign and a small scale) is not settled, nor how strongly the game applies them.

Model textures and bump maps are drawn with their stored mip chains, trilinearly and anisotropically. Drawn (`engine-render`, `model.wgsl`): each vertex carries the direction of increasing u and the sign of v's direction (`tangent::tangents`, from the triangles' UVs), and the normal leans back from the slope, `normalize(n - k (s.u t_u + s.v t_v))` with `k` = 1 (`BUMP_STRENGTH`, by eye). At 4 times that, the Guild's floor grout reads as grooves cut into the tiles, lit on one side and shaded on the other, so the signs are right. Indoors, where most light is ambient, the effect is slight; in sunlight Oakvale's thatch and wall stones stand out.
