//! macOS wallpaper window management via objc2.
//!
//! Creates a borderless, mouse-ignoring window at `kCGDesktopWindowLevel`
//! on each connected screen.  The Drift renderer draws into these windows,
//! replacing the static desktop image with a live fluid animation.
//!
//! On non-macOS platforms this module compiles to no-ops so the crate still
//! builds for CI.

#[cfg(target_os = "macos")]
mod macos {
    use anyhow::{bail, Result};
    use objc2::rc::Retained;
    use objc2_app_kit::{
        NSApplication, NSBackingStoreType, NSScreen, NSWindow, NSWindowCollectionBehavior,
        NSWindowLevel, NSWindowStyleMask,
    };
    use objc2_foundation::{CGPoint, CGRect, CGSize, MainThreadMarker};

    /// `kCGDesktopWindowLevel` from `CGWindowLevel.h`: `INT32_MIN + 5 + 20`.
    const DESKTOP_WINDOW_LEVEL: i64 = i32::MIN as i64 + 5 + 20;

    /// One wallpaper window covering a single display.
    pub struct WallpaperWindow {
        window: Retained<NSWindow>,
    }

    impl WallpaperWindow {
        /// Create a wallpaper window covering `screen`.
        ///
        /// # Safety
        /// Must be called from the main thread while an `NSApplication` is
        /// running.
        pub fn new(mtm: MainThreadMarker, screen: &NSScreen) -> Result<Self> {
            let frame: CGRect = unsafe { screen.frame() };

            let style = NSWindowStyleMask::Borderless;
            let backing = NSBackingStoreType::Buffered;

            let window = unsafe {
                NSWindow::newWithContentRect_styleMask_backing_defer_screen(
                    mtm,
                    frame,
                    style,
                    backing,
                    false,
                    Some(screen),
                )
            };

            // Place the window at the desktop layer.
            unsafe {
                window.setLevel(NSWindowLevel(DESKTOP_WINDOW_LEVEL));
            }

            // Ignore all mouse events so the user can interact with the
            // desktop normally.
            unsafe {
                window.setIgnoresMouseEvents(true);
            }

            // Join all Spaces and prevent Mission Control from treating this
            // window as a separate item.
            let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle;
            unsafe {
                window.setCollectionBehavior(behavior);
            }

            // Prevent the window from gaining key focus.
            unsafe {
                window.setReleasedWhenClosed(false);
                window.orderFrontRegardless();
            }

            Ok(Self { window })
        }

        /// Raw pointer to the underlying `NSWindow` for use with wgpu surface
        /// creation.
        pub fn ns_window_ptr(&self) -> *mut std::ffi::c_void {
            (self.window.as_ref() as *const NSWindow as *mut NSWindow).cast()
        }

        /// Resize the window frame to match the current screen bounds.
        /// Call this when `NSScreenParametersDidChangeNotification` fires.
        pub fn update_frame(&self, screen: &NSScreen) {
            let frame: CGRect = unsafe { screen.frame() };
            unsafe {
                self.window.setFrame_display(frame, false);
            }
        }
    }

    /// Create one [`WallpaperWindow`] per connected display.
    pub fn create_for_all_screens(mtm: MainThreadMarker) -> Vec<WallpaperWindow> {
        let screens = unsafe { NSScreen::screens(mtm) };
        let mut windows = Vec::with_capacity(screens.len());
        for screen in screens.iter() {
            match WallpaperWindow::new(mtm, screen) {
                Ok(w) => windows.push(w),
                Err(e) => log::error!("Failed to create wallpaper window: {e}"),
            }
        }
        windows
    }
}

// Re-export the macOS types when building for macOS.
#[cfg(target_os = "macos")]
pub use macos::{create_for_all_screens, WallpaperWindow};
