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

## Where emitters are placed

- **Mesh dummies.** A mesh's helper names include `CREATEPARTICLE <emitter>` dummies, like the `CREATEOBJECT` ones that place door frames (`mesh.md`): `MESH_INTERNAL_FIREPLACE` has `CREATEPARTICLE FIREPLACE_LOGS_01` 36 units above its origin, next to a `LIGHT_POINT` helper. 160 such dummies in `graphics.big`; the commonest name `BRAZIERFIREFINAL` (27), `CANDLE_FLAME` (20), `STANDARDBRAZIERFIRE` (16).
- **Entities.** A `CParticleAttacherDef` component (26 definitions: lamps, trolls, the dragon) lists emitters by their number in `particles.h`: a u32 count, then per emitter a u32 number, a u32 (an attachment point?), an f32 (10.0 or 20.0) and a byte. Street lamps attach `LAMPGLOW` (255) and `FIRE_FLIES` (254).
