# Architecture and code standard

Status: **adopted** (2026-10-25): entities in `hecs`; feature work pauses until migration step 3. PLAN.md's Architecture section and CLAUDE.md's code rules point here; each migration step below is one session.

## Where the code is now

Four crates. `fable-data` (formats, ~6,900 lines, 100 tests), `engine-audio` (mixer, Ogg) and `engine-script` (quest API, quests) are in good shape and follow the portability rules. Everything else lives in `apps/viewer` (~10,000 lines), which started as an asset viewer and has become the game:

| Viewer module | What it really is | Belongs in |
| --- | --- | --- |
| `walk.rs`, `collide.rs` | navigation grid, routes, floors, collision | engine-world |
| `skin.rs` (posing), `npc.rs` (behaviour), `door.rs` (state), `conversation.rs` | animation sampling, creature AI, interaction, camera framing | engine-world |
| `lighting.rs`, `particles.rs` (simulation), `soundscape.rs` (choice of music and ambience), `story.rs` | time of day and light, particle simulation, audio scene, quest glue | engine-world / game |
| `terrain.rs`, `themes.rs` | building a region from the data (mesh, placements, navigation) | engine-world (scene loading) |
| `render.rs`, `model.rs`, `sky.rs`, `shadow.rs`, `particles.rs` (drawing), `*.wgsl` | GPU resources and passes | engine-render |
| `talk.rs`, HUD in `story.rs` | subtitles, questions, HUD | engine-ui (later) |
| `main.rs` (2,480 lines) | window, input, game loop, quest effects, debug panels, CLI test options | apps/game and apps/viewer |

The symptoms: `ViewerApp` has 49 fields and `TerrainRenderer` 42, most of them game state rather than GPU resources; eight modules mix `wgpu` with game logic; logic advances by each frame's duration (`stable_dt`), not the planned fixed tick; archives are opened in 19 places with no shared cache; 75 `eprintln!` calls serve as logging. None of PLAN.md's `engine-core`, `engine-render`, `engine-ui` or `apps/game` exists.

## Target layout

Dependencies point down only. Nothing above `engine-render` names `wgpu`, a window or the desktop.

```
apps/game        the game: window, input mapping, the loop          (desktop only)
apps/viewer      asset browser and debug tools, on the same crates  (desktop only)
   │
engine-render    GPU resources and passes, drawing a RenderScene    (wgpu backend)
engine-ui        subtitles, questions, HUD, menus                   (draws through engine-render)
   │
engine-world     the simulation: entities, navigation, movement, animation, AI,
                 interaction, conversations, light and time of day, particles,
                 audio scene, region loading                         (portable)
engine-script    quest API and quests, against traits engine-world implements
engine-audio     mixer and decoders                                  (portable)
   │
engine-core      fixed-step time, input actions, events, logging, asset cache (portable)
   │
fable-data       file formats                                        (portable)
```

## The standard

**Layers and portability.** The existing rules stand: `fable-data`, `engine-core`, `engine-world`, `engine-script` and `engine-audio` depend on no GPU, windowing or desktop crate, allocate nothing per frame in steady state, and start no threads beyond the job system. A crate never reaches up the stack.

**The loop.** The simulation advances in fixed 30 Hz ticks (`engine-core::time::Clock`), whatever the frame rate; rendering interpolates between the last two ticks. Input is sampled each frame into actions (move, run, use, attack, target, yes/no) and consumed by the next tick. No game logic reads a frame's duration.

**State.** `engine-world::World` owns the game's state: the Hero, creatures, doors, objects, emitters, lights, the clock, the region. The renderer owns only GPU resources, keyed by asset ids, and each frame draws a `RenderScene` extracted from the world (instances with transforms and poses, lights, sky, particles, camera). Rendering never changes the world.

**Entities.** Entities in a small ECS: `hecs` (minimal, `no_std` with `alloc`, so the Vita port can use it) holding plain components (`Transform`, `Mover`, `Animator`, `Creature`, `Door`, `Emitter`, `Light`, `Role`), with systems as functions over queries, run in a fixed order per tick. Quests reach the world through `engine-script`'s traits, as now.

**Assets.** One `AssetCache` per session opens each archive once and parses each asset once, handing out typed ids (`MeshId`, `TextureId`, `AnimationId`, `EmitterId`). The world holds ids; the renderer turns ids into GPU resources on first use.

**Errors.** Each engine crate has its own error enum (as `fable-data` does); no `Box<dyn Error>` in an engine crate's public API. Apps may box.

**Logging.** The `log` facade in engine crates, by target (`world::walk`, `script::quest`, `audio`); the apps choose the logger and level (`RUST_LOG`). No `eprintln!` outside the apps' command-line output.

**Modules.** One responsibility per module, about 800 lines at most; a file growing past that is split by responsibility, not by size alone.

**Tests.** Every system gets unit tests with synthetic data; real-data tests stay `#[ignore]`d. Simulation tests drive a headless `World` tick by tick (walk the Hero up the Guild's stairs, play a quest's stage), so logic is checked without screenshots; screenshots are for looks.

**Debug tools.** Command-line test options and diagnostics (probes, overlays, censuses) live in `apps/viewer` behind a debug panel or flags, not in the game's code paths.

## Migration

Each step is one session, leaves the game playable, and keeps every test passing.

1. **engine-core** (**done** 2026-10-25): `Clock` (fixed 30 Hz tick), `AssetCache` over the archives, `log` logging. The viewer runs its simulation in ticks (`ViewerApp::tick`, input read once a frame into `Actions`), loads meshes, textures, animations and emitters through the cache, and logs through `log` with `env_logger` (`RUST_LOG`). Left for step 3: interpolating drawn positions between ticks (`Clock::alpha`) once state lives in the world, and the archives still opened directly by the region loader, voices and text.
2. **engine-world, pure parts** (**done** 2026-10-25): navigation and collision (`walk`, `collide`), the camera and conversation framing (`camera`, `conversation`), light and time of day (`lighting`, with `PointLight`), and skeletal animation (`anim`: clips, the animation bank, posing, crossfades, mouth shapes, `Rig` for skeleton and bone-config morph; its `Animator` produces bone matrices, with its own `Error` type) are in `engine-world` (**done**). The viewer keeps CPU deformation (`skin::Animator` wraps the world's and deforms the bind pose). The particle simulation is `engine_world::particles` (`ParticleSystem`: emitters, layers, births and motion, colour over life); the viewer's `particles` keeps the sprite textures, vertex buffer and pipelines.
3. **World and entities**: `World` with `hecs`; the Hero, creatures (behaviour split from `npc.rs` drawing), doors, emitters and lights become entities; region loading (`terrain.rs`) builds a `World`; quest glue (`story.rs`) and the audio scene (`soundscape.rs`) run as systems.
4. **engine-render**: move GPU code and shaders; `RenderScene` extraction; the renderer drops its game state.
5. **apps/game**: the game loop and its window split from the viewer; the viewer keeps the asset browser and debug tools.
6. **engine-ui**: subtitles, questions and HUD out of the apps.

Feature work (the Guild training) pauses during steps 1–3, or continues on the new crates once step 3 is done.
