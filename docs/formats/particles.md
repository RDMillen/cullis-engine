# Particle emitters (`Misc/pc/effects.big`)

Bank `PARTICLE_MAIN_PC`: 1,165 entries, one per emitter, named as in `Defs/RetailHeaders/pc/particles.h` (`TORCHFIRE_01 = 14`, `CANDLE_FLAME = 137`, `FIREPLACE_LOGS_01`). Parser: `fable_data::particles`; every emitter parses to its last byte (real-data test). Sprites are textures in `GBANK_MAIN_PC` (`SPRITE_FLAMELICK_02`, `SPRITE_SMOKE_01`), whose source paths lie under `art3\particles\sprites`.

## Layout

All little-endian.

```
u32 version              100
cstring name             as authored (Fireplace_Logs_01)
36 bytes                 4 zero bytes, 01 00 01, three f32 (12, 18, 12 for CANDLE_FLAME),
                         8 zero bytes, u32 (0 or 2), 5 bytes of flags
u32 layer count          2 for CANDLE_FLAME, 7 for TORCHFIRE_01, 5 for FIREPLACE_LOGS_01
layer count x {
    cstring name         flame, smoke, Fire01
    14 bytes             01 01, three f32 (1.0 in every layer seen)
    u32 component count  3 for sprite layers
    component count x { cstring class, data }
}
```

A component's data starts with a 4-byte hash of its class (`1d d2 d1 8c` for `CPSCRenderSprite`) and a byte 1. Most classes have a fixed size; the others are sized by where the next component starts:

| Class | Uses (emitters) | Data bytes |
| --- | --- | --- |
| `CPSCUpdateNormal` | 4,555 (1,153) | 162 |
| `CPSCEmitterGeneric` | 4,540 (1,150) | 100; 132 or 144 in a few (another hash, `c041477a`, in 9) |
| `CPSCRenderSprite` | 3,266 (1,061) | 109 |
| `CPSCRenderMesh` | 1,222 (536) | 133 |
| `CPSCSpline` | 360 (144) | varies (42, 54, 66, …) |
| `CPSCOrbit` | 205 (89) | 154 |
| `CPSCSingleSprite` | 196 (129) | 142 mostly |
| `CPSCLight` | 151 (147) | 77 |
| `CPSCAttractor` | 47 (26) | varies (32, 44, 56, …) |
| `CPSCDecalRenderer` | 44 (23) | 114 |

A sprite layer is `CPSCRenderSprite` (how it draws), `CPSCEmitterGeneric` (how particles are born) and `CPSCUpdateNormal` (how they move and change).

### `CPSCRenderSprite`

| Offset | Content |
| --- | --- |
| 0 | class hash, then byte 1 |
| 5 | u32 texture (`GBANK_MAIN_PC`): `SPRITE_CANDLE_FLAME_01` (4422) for the candle, `SPRITE_SMOKE_01` (4501) for the torch's smoke |
| 9 | u32 second texture, 0 in most |
| 13 | three colours over a particle's life, 4 bytes each: blue, green, red, alpha. A fireplace layer: (255, 173, 91), (255, 104, 32), (255, 0, 0), alpha 111: yellow-orange to red; another white to black (smoke) |
| 25 | u32 3 or 4 in fires and smoke (a blend mode?) |
| 29– | not decoded: mostly small u32s (2, 0x1f, 0x14, 0x80, 0x94, 0x33, 0x800, 1000) and flag bytes |

### `CPSCUpdateNormal`

Floats seen at these offsets, across chimney smoke, dripping blood, footstep dust, a candle, a fireplace, a small fire and fireflies (meanings not yet known):

| Offset | Values |
| --- | --- |
| 32 | 60 in all |
| 36 | 5, 3.5, 1, 6.3, 0.8, 1, 5 |
| 40 | 0.4, 0, 1, 0, 0, 0.5, 1 |
| 68, 72 | pairs such as (0.05, 0.1), (−0.1, 0.1), (−0.03, 0.03), (0.5, 2): ranges |
| 76 | 0.5 in most |
| 100 | 0, 0.07, 0, 0.1, −0.3, 0.12, 0 |
| 124 | 0.9 or 1 |

`CPSCEmitterGeneric` holds mostly packed integers (`0x0ccd`, `0x1999`, `0x0444`, `1000`); not decoded.

## Readings used for drawing (not confirmed)

Most `CPSCEmitterGeneric` integers read as fixed-point numbers with 1.0 = 1024: the commonest values are 102 (0.1), 205 (0.2), 512 (0.5), 1126 (1.1), 1638 (1.6), 3277 (3.2). The viewer draws sprite layers with these readings (`fable_data::particles::SpriteLayer`), to be checked against the game:

| Reading | From | Fireplace flame, candle |
| --- | --- | --- |
| additive blending, unless mode 2 (alpha) | sprite offset 25 | flames 3 and 4 (sprites on black, no alpha); smoke 2 |
| lifetime in seconds | update offset 36 | 0.8; 6.3 |
| particles born a second | emitter offset 47 as an integer; offset 55 where 47 is 0 | 82; chimney smoke 41 |
| a single particle kept alive | offsets 55 and 82 both 0 | candles, lamp glows, each firefly, blood stains |
| a starting burst | emitter offset 13 | 0 for steady emitters; sword sparks 20, blood 10, footstep dust 5 |
| launch speed and spread from straight up | emitter offsets 43 and 51 (1.0 = 1024) | fireplace 6.4, chimney 1.6, candle 0.16, dust and fireflies 0; spread 0.5 |
| size at birth and death, thousandths of a metre | sprite offsets 45 and 49 | 0.248 → 0; 0.126 → 0.32 |
| acceleration, negative upwards | update offset 100 | −0.3; 0.1 |

Emitter offset 82 (15 for a fireplace flame, 41 for chimney smoke) may be the most particles alive; capping fires to it left them too sparse, so it is not used. The header's first float (10 for the fireplace, 48 for chimney smoke and torches) over 10 is taken as a scale for speeds and sizes, and speeds as 0.1 m/s per unit; both by eye (a fireplace flame then rises about half a metre, chimney smoke about 0.8 m/s). A black middle colour key (torches, candles, smoke) is taken as no middle colour, its alpha still used. Smoke (alpha-blended) reads its colour keys with 128 as 1.0 (the doubled-modulate convention; its grey is 127, and the smoke sprite's own alpha is faint, at most 119) and is lit by the scene's light; flames are self-lit. Layers with `CPSCOrbit`, `CPSCSpline` or `CPSCAttractor` (fireflies, spells) are not drawn yet.

## Smoke generators

A building's `CSmokeGeneratorDef` component holds field `0x76e5195b`, its emitter's number (10, `CHIMNEY_SMOKE_01`, on 318 of 323 buildings; 64, `CAMPFIRE_01`, on 5), and field `0x54a805b2` (32.0 in all). The smoke rises from the mesh's `focus_chimney_01` helpers (`mesh.md`): one at the top of each Oakvale house, two on some.

## Where emitters are placed

- **Mesh dummies** (`Skeleton::created_particles`; the viewer starts these). A mesh's helper names include `CREATEPARTICLE <emitter>` dummies, like the `CREATEOBJECT` ones that place door frames (`mesh.md`): `MESH_INTERNAL_FIREPLACE` has `CREATEPARTICLE FIREPLACE_LOGS_01` 36 units above its origin, next to a `LIGHT_POINT` helper. 160 such dummies in `graphics.big`; the commonest name `BRAZIERFIREFINAL` (27), `CANDLE_FLAME` (20), `STANDARDBRAZIERFIRE` (16).
- **Entities.** A `CParticleAttacherDef` component (26 definitions: lamps, trolls, the dragon) lists emitters by number (`fable_data::particles::parse_attachments`): `01 00 01`, field `0x3696f44d`, a u32 count, then 13 bytes each: u32 emitter number (its bank entry id), u32 byte offset in `names.bin` of the mesh helper it sits at (`FLARE_POINT`, `LIGHT_POINT`, `EMITTER_LOWERARM`, `DRAGON_MOUTH_DUMMY`; `0xffffffff` for none), f32 (10.0 or 20.0), u8 (0 on lamps, 1 on creatures). Street lamps attach `LAMPGLOW` (255) at `FLARE_POINT` and `FIRE_FLIES` (254) at `LIGHT_POINT`. Every attachment's helper name resolves (real-data test).
