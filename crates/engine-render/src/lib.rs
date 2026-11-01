//! Rendering: GPU resources and passes that draw a region's world (terrain,
//! placed meshes, creatures, doors and dummies, the Hero, sky, shadows, particles), on
//! wgpu. Nothing here changes the world; see `docs/architecture.md`.

pub mod animated;
pub mod creature;
pub mod model;
pub mod particles;
pub mod render;
pub mod shadow;
pub mod skin;
pub mod sky;
pub mod tangent;
pub mod terrain;
pub mod themes;
pub mod weapon;
