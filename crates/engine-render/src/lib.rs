//! Rendering: GPU resources and passes that draw a region's world (terrain,
//! placed meshes, creatures, doors, the Hero, sky, shadows, particles), on
//! wgpu. Nothing here changes the world; see `docs/architecture.md`.

pub mod creature;
pub mod door;
pub mod model;
pub mod particles;
pub mod render;
pub mod shadow;
pub mod skin;
pub mod sky;
pub mod terrain;
pub mod themes;
