//! The simulation: the [`world`] of a region, skeletal animation ([`anim`]), where things can walk and how they get there ([`walk`]),
//! what they bump into ([`collide`]), the camera ([`camera`]) and how
//! conversations are framed ([`conversation`]), and light by time of day
//! ([`lighting`]).
//!
//! No rendering, windowing or platform dependency, so ports can reuse it; see
//! `docs/architecture.md`.

pub mod anim;
pub mod camera;
pub mod collide;
pub mod conversation;
pub mod lighting;
pub mod particles;
pub mod walk;
pub mod world;
