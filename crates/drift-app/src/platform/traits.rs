//! Platform trait definitions.
//!
//! These traits define the interface for platform-specific functionality.
//! Each platform implementation must provide implementations for all traits.

use std::sync::{Arc, RwLock};

use winit::monitor::MonitorHandle;
use winit::window::Window;

use crate::config::AppConfig;
use crate::media_art::NowPlayingSnapshot;
use drift_core::NowPlayingSource;

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

/// Trait for system tray integration.
pub trait SystemTray: Send + Sync {
    type TrayHandle;

    /// Create a system tray icon with the given menu.
    fn create_tray(config: Arc<RwLock<AppConfig>>) -> anyhow::Result<Self::TrayHandle>
    where
        Self: Sized;
}

/// Trait for native color picker dialog.
pub trait NativeColorPicker: Send + Sync {
    /// Open the native color picker and return the selected color.
    fn pick_color(initial: Option<[u8; 3]>) -> anyhow::Result<Option<[u8; 3]>>;
}

/// Trait for autostart/login item management.
pub trait AutostartManager: Send + Sync {
    /// Install autostart (run at login).
    fn install() -> anyhow::Result<()>;

    /// Remove autostart.
    fn uninstall() -> anyhow::Result<()>;

    /// Check if autostart is currently installed.
    fn is_installed() -> bool;
}

/// Trait for querying now playing / media art from media players.
pub trait MediaQuerier: Send + Sync {
    /// Query the current now playing info from the specified source.
    fn query_now_playing(
        source: NowPlayingSource,
        previous_key: Option<&str>,
    ) -> anyhow::Result<Option<NowPlayingSnapshot>>;
}

/// Screensaver mode when running as a screensaver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreensaverMode {
    /// Not running as screensaver.
    None,
    /// Running as fullscreen screensaver.
    Fullscreen,
    /// Running as preview in a window (screensaver config preview).
    Preview { parent_hwnd: usize },
    /// Open configuration dialog.
    Configure,
}

/// Trait for screensaver integration.
pub trait ScreensaverRunner: Send + Sync {
    /// Parse command line arguments for screensaver mode.
    fn parse_screensaver_args() -> ScreensaverMode;

    /// Configure the system to use this app as the screensaver.
    fn configure_as_screensaver() -> anyhow::Result<()>;
}
