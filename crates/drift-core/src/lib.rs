//! drift-core: platform-independent fluid-simulation rendering for drift-wallpaper-macos.
//!
//! This crate provides:
//! - [`DriftRenderer`]: a wgpu-based renderer for the Drift fluid aesthetic.
//! - [`DriftParams`]: configurable simulation parameters (speed, scale, colours).
//! - [`ColorPalette`]: colour preset library and image-based colour extraction.

pub mod color;
pub mod renderer;
pub mod simulation;

pub use color::{ColorPalette, Preset};
pub use renderer::DriftRenderer;
pub use simulation::DriftParams;
