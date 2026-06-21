//! Platform trait definitions.

use winit::monitor::MonitorHandle;
use winit::window::Window;

/// Marker trait for wallpaper manager that syncs after window creation.
/// On macOS, AppKit may resize windows after creation, so we need to sync.
pub trait WallpaperManagerSync: Send + Sync {
    const REQUIRES_SYNC: bool;
}

/// Trait for wallpaper window management.
///
/// Handles creating and managing wallpaper windows at the correct
/// desktop level for each platform.
pub trait WallpaperManager: Send + Sync {
    /// Set the wallpaper window level (desktop level, ignores mouse events).
    fn set_desktop_level(window: &Window) -> anyhow::Result<()>;

    /// Snap a wallpaper window to a specific monitor's bounds.
    fn snap_to_monitor(window: &Window, monitor: &MonitorHandle) -> anyhow::Result<()>;

    /// Snap a wallpaper window to span all monitors.
    fn snap_to_all_monitors(window: &Window) -> anyhow::Result<()>;

    /// Sync renderer size after window frame changes.
    fn sync_renderer_size(
        window: &Window,
        renderer: &mut drift_core::FluxRenderer,
    ) -> anyhow::Result<()>;
}
