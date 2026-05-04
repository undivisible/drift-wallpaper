//! Platform abstraction layer for drift-wallpaper.
//!
//! This module provides a unified interface for platform-specific functionality
//! that differs between macOS, Windows, and Linux.
#![allow(dead_code, unused_imports)]

pub mod traits;

pub use traits::*;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(any(target_os = "windows", target_os = "linux"))]
pub mod detail;
