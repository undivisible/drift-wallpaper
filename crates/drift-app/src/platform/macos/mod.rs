//! macOS platform implementation.

pub mod wallpaper;
pub mod tray;
pub mod color_picker;
pub mod autostart;
pub mod media;
pub mod screensaver;

pub use wallpaper::MacosWallpaperManager;
pub use tray::MacosSystemTray;
pub use color_picker::MacosColorPicker;
pub use autostart::MacosAutostart;
pub use media::MacosMediaQuerier;
pub use screensaver::MacosScreensaverRunner;

use crate::platform::{WallpaperManager, WallpaperManagerSync};

impl WallpaperManagerSync for MacosWallpaperManager {
    const REQUIRES_SYNC: bool = true;
}

impl WallpaperManager for MacosWallpaperManager {
    fn set_desktop_level(window: &winit::window::Window) -> anyhow::Result<()> {
        crate::set_desktop_window_level(window);
        Ok(())
    }

    fn snap_to_monitor(
        window: &winit::window::Window,
        monitor: &winit::monitor::MonitorHandle,
    ) -> anyhow::Result<()> {
        crate::macos_snap_wallpaper_window_to_monitor(window, monitor);
        Ok(())
    }

    fn snap_to_all_monitors(window: &winit::window::Window) -> anyhow::Result<()> {
        crate::macos_snap_wallpaper_window_to_union_of_screens(window);
        Ok(())
    }

    fn sync_renderer_size(
        window: &winit::window::Window,
        renderer: &mut drift_core::FluxRenderer,
    ) -> anyhow::Result<()> {
        crate::sync_flux_renderer_to_wallpaper_window(window, renderer);
        Ok(())
    }
}
