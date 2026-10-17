# Environment themes and the lighting table

Light by time of day. Parser: `fable_data::environment`; used by the viewer's `lighting.rs` and `sky.rs`.

## Regions

A `REGION` definition record starts with the 5-byte header, then:

| Field hash | Name | Value |
| --- | --- | --- |
| `0x4a59983b` | `EnvironmentTheme` | definition index of its outdoor theme (`ENVIRONMENT_THEME1` for most regions, including the Prologue's Oakvale; `ENVIRONMENT_OAKVALE` for `REGION_OAK_VALE`) |
| `0x98e5785a` | `InternalEnvironmentTheme` | definition index of its indoor theme (`ENVIRONMENT_INTERNALS` for most) |
| `0x27945c1b` | not known | 1 in every region except `REGION_HERO_GUILD_COMPLEX_INSIDE` (2); taken to mean "lit by the internal theme" |

Names come from hashing guesses with the definitions' name hash (`defs.md`); a name found this way among millions of guesses is unlikely to be a coincidence (37 hashes, about 0.05 expected false matches).

`ENVIRONMENT_OV_POSTATTACK` (Oakvale after the raid) is referred to by no region; the quest state switches to it. Which part of the game does that is not known, so the viewer picks it when the region loads with the after-the-raid section.

## Themes (`ENVIRONMENT_THEME_DAY`)

39 definitions. Record layout, all little-endian:

```
5 bytes            header (01 00 01 00 00; zeros in the null definition)
u32 0xeee2e867     Time
u32 count          keyframes
count x keyframe
theme fields       (u32 hash, u32 value) pairs to the end
```

A keyframe is `u32 0xb400c903` (`TimeOfDay`) and its `f32` hour, then about 35 (hash, value) fields. Values are 4 bytes except field `0x8632d8f5` (`MoonLit`), which is one byte (0 or 1; 1 at Oakvale's and the Guild's night keyframes). Every keyframe of a theme has the same fields in the same order. Hours ascend (Oakvale's: 0, 4, 5, 12, 16, 20, 21) and wrap round midnight.

| Keyframe field | Name | Observed |
| --- | --- | --- |
| `0x425d252f` | `SkyTexture0` | a `GBANK_MAIN_PC` texture id: `GRAPHIC_ATMOSPHERIC_SKY_MIDNIGHT`, `_MORNING_BRIGHTWOOD`, `_EVENING_BRIGHTWOOD`, … |
| `0x355a15b9` | `SkyTexture1` | a second sky texture, or 0 |
| `0x15740d29` | `SkyTexture1Blend` | 0 or 0.5 where `SkyTexture1` is set: its share of the blend |
| `0xf88a218f`, `0x020037a7` | not known | cloud textures (`SKY_BRIGHTWOOD_CLOUDS_02`, `_01`; `SKY_RAIN_CLOUDS_*` after the raid): DXT3 with alpha, tiling. Each is followed by three values: (0, 0, 1) after the first, (0, 0, 0.5) after the second in every theme seen; the third is taken as the layer's opacity (`0x111cfb1a`, `0x2a4c3397`) |
| `0x3b3168b3` | not known | 0.3–0.7 through Oakvale's day, 0 after the raid |
| `0xbf2cb1ac` | `WaterAlphaFactor` | 1 or 0.9 |
| `0x91c70efd` | not known | 60–200 (Oakvale 100, 200 at 16:00) |
| `0x7a493bd2` | `MistAlpha` | 0, 0.2 after the raid |
| `0xb2a4a400`, `0xb78498c4` | `RainStrength`, `SnowStrength` | 0 in the themes seen |
| `0x3c1b162f` | `LightningFrequency` | 0 in the themes seen |
| `0xe50a5440` | `ShadowFactor` | 0 outdoors, 0.5 indoors |
| `0x360040b2` | `GlowBloomScale` | 0.2–0.4 |
| others | not known | constant within the themes seen (0.5, 0.8, 1.0, 15, 0.25, 0.05, 1/16, 1/24, 0.01, …) |

| Theme field | Name | Observed |
| --- | --- | --- |
| `0x12a7b130` | `ColourLookupColumn` | the theme's first column in the lighting table (36 for Oakvale, 101 after the raid, 129 for the Guild) |
| `0x37c59465`, `0x79ff0650` | `SunHeight`, `MoonHeight` | 0.2, 0.9 |
| `0xf6da2a77` | not known | 4 bytes, a colour? (ff 80 80 ff in Oakvale) |
| `0xf2995fa0`, `0xf5012366`, `0x8d8f6647`, `0xbb3bb153`, `0x08c6a844` | not known | 0.18, 0.25, -0.4, 0.35, 1.0 |

## Sky textures

The sky textures are 512x512 panoramas: zenith at the top, horizon at the bottom, a glow in the middle on the morning and evening skies. The viewer draws them behind everything, mapping elevation to the vertical and the angle from the sun to the horizontal (a choice; how the game maps them is not known). Between keyframes, each keyframe's `SkyTexture0` and `SkyTexture1` (by its blend share) are blended by time.

## Clouds

The viewer draws the two cloud layers over the sky, projected onto a plane overhead (fading towards the horizon), each texture's alpha times its opacity, coloured by row 15 and drifting slowly. The projection, tiling and drift speeds are chosen by eye; how the game draws them is not known. After the raid the cloud row is black and the sky texture is the dark midnight one under an orange tint, so that sky stays nearly black.

## Lighting table (`LightingTable/lighting_colours.tga`)

An uncompressed 24-bit TGA, 190x21, stored bottom row first. Columns come in runs separated by magenta (ff00ff) columns; each run is one theme, one column per keyframe: the run starting at a theme's `0x12a7b130` value is exactly as long as its keyframe count (checked for every theme by a real-data test). Each of the 21 rows is a colour; magenta cells are unused (indoor themes fill rows 0, 1, 3, 4, 6 and 7 only).

What each row lights is not recorded. From how Oakvale's colours change through the day:

| Row | Oakvale 00:00 → 12:00 → 20:00 | Taken as |
| --- | --- | --- |
| 0 | 3e3e3e → 9d8060 → 875a30 | ambient light indoors; outdoors, averaged with row 1 (filled indoors too) |
| 7 | cccccc → b0b0b0 → b2b2b2; indoors cccccc → e0e0e0 | near white in every theme; used indoors as the light coming in through windows |
| 1 | 101836 → 46405f → 4f2b34 | a cool fill from the sky, averaged with row 0 outdoors (very dark brown indoors, so unused there) |
| 5 | 0b1626 → b8a9a8 → bb9163 | (sky near the horizon?) |
| 8 | 14181e → ffffff → f4ce9d | the sun's light |
| 9 | ebebeb → 6186b4 → 87a2c4 | the sky texture's tint (blue over the warm day texture, near white over the midnight one) |
| 10, 14, 16 | bright at night, black by day | (moon, stars?) |
| 11 | 595959 → ffffff → ffe0b7 | (sun disc?) |
| 15 | 13111a → a7a4a9 → 855436 | the clouds' colour (black after the raid) |

After the raid, rows 8–12 are all orange (744326), row 0 dark red (6c2119) and row 20 deep red (8c0002).

The viewer lights terrain and models with `ambient + sun * max(n·l, 0) * sunlit`, where `sunlit` comes from a shadow map rendered from the sun (outdoors only; the game's own shadowing is not decoded). Outdoors the ambient is the average of rows 0 and 1: row 0 alone tinted every shaded surface orange-brown, row 1 alone lavender (the average is by eye). The sum is added in gamma space and clamped (as fixed-function lighting did), then scaled by a brightness factor and capped at a highlight limit (1.3 by default, by eye: the strongest channel is held to it, keeping the light's colour, so sunlit faces do not wash out), (2.1 by default, matched against the original; adjustable in the window) so light can go past 1, as the game's dim noon ambient (about 60%) suggests overbright lighting, then converted to linear. Indoor themes leave the sun's row unused and their ambient is dim (about 30% grey), so indoors the viewer lights from above on the sun's side with row 7 at half strength (by eye), standing for daylight through windows and doors. The sun rises in the east at 06:00, is highest in the south at noon and sets at 18:00; at night the same arc stands for the moon. That arc is a choice, not from the data.
