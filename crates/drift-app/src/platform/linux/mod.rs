//! Linux platform implementation.

pub mod wallpaper;

pub use wallpaper::LinuxWallpaperManager;

use crate::platform::{WallpaperManager, WallpaperManagerSync};

impl WallpaperManagerSync for LinuxWallpaperManager {
    const REQUIRES_SYNC: bool = false;
}

impl WallpaperManager for LinuxWallpaperManager {
    fn set_desktop_level(window: &winit::window::Window) -> anyhow::Result<()> {
        Self::set_desktop_level(window)
    }

    fn snap_to_monitor(
        window: &winit::window::Window,
        monitor: &winit::monitor::MonitorHandle,
    ) -> anyhow::Result<()> {
        Self::snap_to_monitor(window, monitor)
    }

    fn snap_to_all_monitors(window: &winit::window::Window) -> anyhow::Result<()> {
        Self::snap_to_all_monitors(window)
    }

    fn sync_renderer_size(
        window: &winit::window::Window,
        renderer: &mut drift_core::FluxRenderer,
    ) -> anyhow::Result<()> {
        Self::sync_renderer_size(window, renderer)
    }
}
