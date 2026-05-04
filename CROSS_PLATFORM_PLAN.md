# Cross-Platform Porting Plan: drift-wallpaper

## Executive Summary

This document outlines a comprehensive plan to port drift-wallpaper from macOS-only to Windows and Linux, including screensaver hooks for both platforms.

**Current State**: drift-wallpaper is a macOS-only live wallpaper app with ~59 `#[cfg(target_os = "macos")]` guards across the codebase.

**Target Platforms**:
- Windows 10/11 (x64)
- Linux (X11 primary, Wayland secondary via xdg-desktop-portal)
- macOS (existing, refactored)

---

## Part 1: Architecture Analysis

### Current Code Structure

```
drift-app/src/
├── main.rs          # Entry point, event loop, macOS window level (674-792)
├── wallpaper.rs     # ENTIRELY macOS - NSWindow at desktop level
├── menubar.rs       # ENTIRELY macOS - NSStatusBar tray
├── color_picker.rs  # ENTIRELY macOS - NSColorPanel
├── launch_agent.rs  # ENTIRELY macOS - launchd plist
├── now_playing.rs   # PARTIAL - Spotify observer via NSDistributedNotificationCenter
├── media_art.rs     # PARTIAL - osascript for Spotify/Apple Music
├── cli.rs           # PARTIAL - macOS screencapture/sips commands
├── config.rs        # PARTIAL - config path differs per platform
├── ui.rs            # MOSTLY cross-platform - macOS menu bar tray suppression
└── crepus_*.rs      # ENTIRELY cross-platform
```

### drift-core (Platform-Agnostic)
- 99% cross-platform
- Only `renderer.rs` has minor macOS-specific `sips` wallpaper conversion

---

## Part 2: Proposed Architecture

### Platform Abstraction Layer

Introduce a `platform` module with traits for platform-specific functionality:

```rust
// crates/drift-app/src/platform/mod.rs

pub mod wallpaper;    // Wallpaper window management
pub mod tray;         // System tray
pub mod color_picker; // Native color picker
pub mod autostart;    // Login item / autostart
pub mod media;        // Now playing / media art
pub mod screensaver;  // Screensaver integration
```

### Trait Definitions

```rust
// platform/wallpaper.rs
pub trait WallpaperManager {
    fn set_desktop_level(window: &Window) -> Result<()>;
    fn set_window_ignore_mouse(window: &Window, ignore: bool) -> Result<()>;
    fn snap_to_monitor(window: &Window, monitor: &MonitorHandle) -> Result<()>;
    fn snap_to_all_monitors(window: &Window) -> Result<()>;
    fn create_wallpaper_windows() -> Vec<WallpaperWindowHandle>;
}

// platform/tray.rs
pub trait SystemTray {
    fn create_tray_item(config: Arc<Mutex<AppConfig>>) -> Result<Box<dyn TrayHandle>>;
    fn update_tray_state(&self, state: TrayState) -> Result<()>;
}

// platform/color_picker.rs
pub trait NativeColorPicker {
    fn pick_color(initial: Option<Color>) -> Result<Option<Color>>;
}

// platform/autostart.rs
pub trait AutostartManager {
    fn install() -> Result<()>;
    fn uninstall() -> Result<()>;
    fn is_installed() -> bool;
}

// platform/media.rs
pub trait MediaPlayer {
    fn query_now_playing(source: NowPlayingSource) -> Result<Option<NowPlayingSnapshot>>;
    fn spawn_observer() -> Result<Box<dyn MediaObserver>>;
}

// platform/screensaver.rs
pub trait Screensaver {
    fn configure_as_screensaver() -> Result<()>;
    fn handle_screensaver_args() -> ScreensaverMode;
}
```

---

## Part 3: Windows Implementation

### Dependencies (Cargo.toml additions)

```toml
[target.'cfg(target_os = "windows")'.dependencies]
windows = { version = "0.58", features = [
    "Win32_Foundation",
    "Win32_UI_WindowsAndMessaging",
    "Win32_UI_Shell",
    "Win32_System_Registry",
    "Win32_Media_MediaPlayer",
] }
tray-icon = "0.23"  # Or windows crate for raw Shell_NotifyIcon
image = { version = "0.25", features = ["png", "jpeg", "bmp"] }

# MPRIS via D-Bus (optional, for Windows Media Session)
# zbus = "4"

[target.'cfg(target_os = "linux")'.dependencies]
x11 = { version = "2", features = ["xrandr", "xrender", "xtest"] }
xcb = "0.10"
tray-icon = "0.23"
# MPRIS via D-Bus
zbus = "4"
dbus-crossroads = "0.5"
image = { version = "0.25", features = ["png", "jpeg"] }

[target.'cfg(target_os = "windows")'.dependencies]
libXScrnSaver = "0.4"  # If available, else native implementation
```

### Windows Wallpaper Window Implementation

Windows approach: Use layered windows with `WS_EX_NOACTIVATE` and `WS_EX_TOOLWINDOW` to create a borderless window that doesn't interfere with desktop interaction.

```rust
// platform/detail/windows/wallpaper.rs

use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Gdi::*;

pub struct WindowsWallpaperManager;

impl WallpaperManager for WindowsWallpaperManager {
    fn set_desktop_level(window: &Window) -> Result<()> {
        // Use SetWindowPos with HWND_BOTTOM to place below desktop icons
        // Use WS_EX_NOACTIVATE to prevent activation
        // Use WS_EX_TOOLWINDOW to hide from alt-tab
        unsafe {
            let hwnd = get_hwnd(window);
            SetWindowPos(hwnd, HWND_BOTTOM, 0, 0, 0, 0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)?;
            SetWindowLongW(hwnd, GWL_EXSTYLE,
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW)?;
        }
        Ok(())
    }
}
```

### Windows Screensaver Implementation

Windows screensavers are `.scr` files (renamed executables). Key approach:
1. Add a `/s` flag handler (show screensaver preview)
2. Add a `/p <hwnd>` flag handler (preview in window)
3. Add a `/c` flag handler (configure)

```rust
// For Windows, main() parses command line for:
// /s - Run as screensaver (fullscreen on primary monitor)
// /p <hwnd> - Preview in window handle
// /c - Configure (open settings UI)

pub enum ScreensaverMode {
    None,
    Fullscreen,
    Preview { parent_hwnd: isize },
    Configure,
}
```

---

## Part 4: Linux Implementation

### Linux Wallpaper Window Implementation

X11 approach: Create borderless override-redirect window at desktop level:

```rust
// platform/detail/linux/wallpaper.rs

use x11::xlib::{Display, Window, XInternAtom, XDefaultRootWindow};
use x11::xrandr::{XRRScreenResources, XRRGetScreenResources};

pub struct LinuxWallpaperManager {
    display: *mut Display,
}

impl WallpaperManager for LinuxWallpaperManager {
    fn set_desktop_level(window: &Window) -> Result<()> {
        // Use _NET_WM_WINDOW_TYPE_DESKTOP from EWMH
        // Or use override-redirect window below desktop
    }
}
```

For Wayland: Use `xdg-desktop-portal` or `wlr-layer-shell` (if compositor supports).

### Linux Screensaver Implementation

Linux screensavers typically use `libXScrnSaver`:

```rust
// platform/detail/linux/screensaver.rs

use xss::ScreenSaver;

pub struct LinuxScreensaver;

impl Screensaver for LinuxScreensaver {
    fn configure_as_screensaver() -> Result<()> {
        // Register with libXScrnSaver
        // Or write .desktop file for xscreensaver integration
    }
}
```

Also add support for:
- `xdg-screensaver` protocol
- Integration with GNOME Screensaver
- Integration with KDE Screen Saver

### Linux Autostart

Create `.desktop` file in `~/.config/autostart/`:

```desktop
[Desktop Entry]
Type=Application
Name=Drift Wallpaper
Exec=/path/to/drift-wallpaper --background
Hidden=false
X-GNOME-Autostart-enabled=true
```

---

## Part 5: Media Art / Now Playing

### Current macOS Implementation
- Spotify: Uses `NSDistributedNotificationCenter` + osascript
- Apple Music: Uses osascript

### Cross-Platform Solution: MPRIS D-Bus

Use MPRIS (Media Player Remote Interchange Specification) via `zbus`:

```rust
// platform/media/mpris.rs

use zbus::Connection;
use zbus::proxy::Proxy;

pub struct MprisMediaPlayer;

impl MediaPlayer for MprisMediaPlayer {
    fn query_now_playing(source: NowPlayingSource) -> Result<Option<NowPlayingSnapshot>> {
        match source {
            NowPlayingSource::Automatic | NowPlayingSource::Spotify => {
                self.query_spotify_via_mpris()
            }
            NowPlayingSource::AppleMusic => {
                self.query_apple_music_via_mpris()
            }
        }
    }
}
```

**MPRIS Service Names**:
- Spotify: `org.mpris.MediaPlayer2.spotify`
- Apple Music (non-Linux): Use Windows Media Session API or Fallback to osascript on macOS
- Browser media: `org.mpris.MediaPlayer2.browsertabs`

**Note**: MPRIS on Windows requires D-Bus daemon (WSLg or manual installation), so for native Windows use Windows Media Session API.

---

## Part 6: Implementation Phases

### Phase 1: Refactoring (1-2 weeks)
- [ ] Extract `platform` module with trait definitions
- [ ] Move macOS implementations behind traits
- [ ] Create `#[cfg]` stubs for non-macOS
- [ ] Update build system for multi-platform

### Phase 2: Windows Core (2-3 weeks)
- [ ] Implement Windows wallpaper window
- [ ] Implement Windows system tray
- [ ] Implement Windows config path
- [ ] Test wallpaper creation/destruction

### Phase 3: Windows Features (1-2 weeks)
- [ ] Implement Windows color picker
- [ ] Implement Windows autostart (Registry)
- [ ] Implement Windows screensaver hook
- [ ] Add Windows Media Session API for now playing

### Phase 4: Linux Core (2-3 weeks)
- [ ] Implement X11 wallpaper window
- [ ] Implement Linux system tray (libappindicator)
- [ ] Implement XDG autostart
- [ ] Test multi-monitor support

### Phase 5: Linux Features (1-2 weeks)
- [ ] Implement Linux color picker (zenity/GTK)
- [ ] Implement libXScrnSaver screensaver hook
- [ ] Add MPRIS via zbus for media art
- [ ] Wayland support (if time permits)

### Phase 6: Polish (1 week)
- [ ] Bug fixes across platforms
- [ ] CI/CD for Windows and Linux builds
- [ ] Documentation for each platform
- [ ] Release packaging (.msi, .deb, .AppImage)

---

## Part 7: Reference Implementations

### Lively Wallpaper (Windows, C#)
- GitHub: https://github.com/rocksdanister/lively
- Key approach: WinUI 3, wallpaper as separate windows
- Screensaver: `.scr` file that wraps the app

### Cosmic Widgets (Linux, Rust)
- GitHub: https://github.com/pop-os/cosmic-widgets
- Key approach: COSMIC desktop integration
- May provide patterns for system tray and wallpaper

### Useful Crates

| Crate | Purpose |
|-------|---------|
| `zbus` | D-Bus / MPRIS on Linux |
| `tray-icon` | Cross-platform system tray |
| `windows` | Win32 API bindings |
| `x11` | X11 bindings |
| `xcb` | X11 protocol |
| `libXScrnSaver-sys` | Screen saver integration |
| `notify-rust` | File watching (config changes) |
| `image` | Image loading for artwork |

---

## Part 8: Testing Strategy

### Build Testing
```bash
# Windows (cross-compile from Linux)
cargo build --release --target x86_64-pc-windows-msvc

# Linux
cargo build --release --target x86_64-unknown-linux-gnu

# macOS (existing)
cargo build --release
```

### Runtime Testing
1. Multi-monitor detection
2. Wallpaper window level correctness
3. System tray functionality
4. Color picker integration
5. Autostart registration/removal
6. Screensaver preview mode
7. Now playing media detection

### CI/CD Additions
- GitHub Actions: Windows build (`x86_64-pc-windows-msvc`)
- GitHub Actions: Linux build (Ubuntu latest)
- GitHub Actions: Test on PR

---

## Part 9: Potential Challenges

1. **Window level on Windows**: Finding the correct `HWND` to place wallpaper below is non-trivial
2. **Wayland**: Full Wayland support requires compositor-specific code or portals
3. **Media player detection**: MPRIS not universally implemented (Chrome, Firefox tabs don't use it)
4. **Screensaver on Linux**: Multiple incompatible standards (xscreensaver, gnome-screensaver, KDE)
5. **D-Bus on Windows**: Not natively available

---

## Part 10: Deliverables

1. Cross-platform `drift-wallpaper` binary for Windows and Linux
2. `.scr` file for Windows screensaver integration
3. `.desktop` file for Linux screensaver integration (xscreensaver)
4. Multi-platform CI/CD pipeline
5. User documentation for Windows and Linux installation

---

## Appendix: File Structure After Refactoring

```
crates/drift-app/src/
├── main.rs
├── lib.rs                    # New: library entry for screensaver mode
├── platform/
│   ├── mod.rs               # Trait definitions
│   ├── dummy.rs             # Stub implementations (no-op)
│   ├── macos/
│   │   ├── mod.rs
│   │   ├── wallpaper.rs     # Current wallpaper.rs
│   │   ├── tray.rs          # Current menubar.rs
│   │   ├── color_picker.rs  # Current color_picker.rs
│   │   ├── autostart.rs     # Current launch_agent.rs
│   │   ├── media.rs         # macOS media (osascript)
│   │   └── screensaver.rs   # macOS screensaver (if applicable)
│   ├── windows/
│   │   ├── mod.rs
│   │   ├── wallpaper.rs     # Win32 wallpaper windows
│   │   ├── tray.rs          # Shell_NotifyIcon
│   │   ├── color_picker.rs   # ChooseColor
│   │   ├── autostart.rs      # Registry Run key
│   │   ├── media.rs          # Windows Media Session API
│   │   └── screensaver.rs    # /s /p /c handling
│   └── linux/
│       ├── mod.rs
│       ├── wallpaper.rs      # X11/Wayland wallpaper
│       ├── tray.rs           # libappindicator
│       ├── color_picker.rs    # zenity/GTK
│       ├── autostart.rs      # .desktop file
│       ├── media.rs          # MPRIS via zbus
│       └── screensaver.rs    # libXScrnSaver
├── ui.rs
├── config.rs
├── cli.rs
├── media_art.rs
├── now_playing.rs
├── crepus_*.rs
└── color_picker.rs           # May become cross-platform
```

---

*Document Version: 1.0*
*Created: 2026-04-14*
