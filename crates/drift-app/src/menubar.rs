//! macOS menubar status item implemented with objc2.
//!
//! Provides a 🌊 icon in the system menu bar with the following options:
//! - Enable / Disable wallpaper
//! - Select colour preset (Ocean, Sunset, Forest, Lava, Midnight, Monochrome)
//! - Upload an image to extract colours
//! - Start at Login toggle
//! - Quit

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::{Arc, Mutex};

    use objc2::rc::Retained;
    use objc2::runtime::Sel;
    use objc2::MainThreadOnly;
    use objc2_app_kit::{
        NSMenu, NSMenuItem, NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
    };
    use objc2_foundation::{ns_string, MainThreadMarker, NSString};

    use crate::config::AppConfig;

    /// Shared application config, protected by a mutex so the menu callbacks
    /// can mutate it.
    pub type SharedConfig = Arc<Mutex<AppConfig>>;

    /// Create the system status bar item and populate its menu.
    ///
    /// Returns the `NSStatusItem` (must be retained for the lifetime of the
    /// app – if it is dropped the icon disappears).
    pub fn create_status_item(
        mtm: MainThreadMarker,
        config: SharedConfig,
    ) -> Retained<NSStatusItem> {
        let status_bar = NSStatusBar::systemStatusBar();
        let status_item = status_bar.statusItemWithLength(NSVariableStatusItemLength);

        // Set the icon text (emoji as a quick placeholder).
        if let Some(button) = status_item.button(mtm) {
            let title = NSString::from_str("🌊");
            button.setTitle(&title);
        }

        let menu = build_menu(mtm, config);
        status_item.setMenu(Some(&menu));

        status_item
    }

    // -----------------------------------------------------------------------
    // Menu construction
    // -----------------------------------------------------------------------

    fn build_menu(mtm: MainThreadMarker, config: SharedConfig) -> Retained<NSMenu> {
        let menu = NSMenu::new(mtm);

        // ── Enable / Disable ───────────────────────────────────────────────
        let enabled = config.lock().unwrap().enabled;
        let toggle_item = make_item(
            mtm,
            if enabled {
                "Disable Wallpaper"
            } else {
                "Enable Wallpaper"
            },
            None,
        );
        menu.addItem(&toggle_item);

        separator(&menu, mtm);

        // ── Colour presets submenu ─────────────────────────────────────────
        let presets_item = make_item(mtm, "Colour Preset", None);
        let presets_menu = NSMenu::new(mtm);
        for &preset in drift_core::color::Preset::all() {
            let item = make_item(mtm, preset.label(), None);
            presets_menu.addItem(&item);
        }
        presets_item.setSubmenu(Some(&presets_menu));
        menu.addItem(&presets_item);

        // ── Upload image ───────────────────────────────────────────────────
        let upload_item = make_item(mtm, "Extract Colors from Image…", None);
        menu.addItem(&upload_item);

        separator(&menu, mtm);

        // ── Launch at Login ────────────────────────────────────────────────
        let login = config.lock().unwrap().launch_at_login;
        let login_item = make_item(
            mtm,
            if login {
                "✓ Start at Login"
            } else {
                "Start at Login"
            },
            None,
        );
        menu.addItem(&login_item);

        separator(&menu, mtm);

        // ── Quit ───────────────────────────────────────────────────────────
        let quit_item = make_item(mtm, "Quit Drift Wallpaper", Some(objc2::sel!(terminate:)));
        menu.addItem(&quit_item);

        menu
    }

    fn make_item(mtm: MainThreadMarker, title: &str, action: Option<Sel>) -> Retained<NSMenuItem> {
        let title_ns = NSString::from_str(title);
        let key = ns_string!("");
        unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &title_ns,
                action,
                key,
            )
        }
    }

    fn separator(menu: &NSMenu, mtm: MainThreadMarker) {
        let sep = NSMenuItem::separatorItem(mtm);
        menu.addItem(&sep);
    }
}

// Re-export for macOS.
#[cfg(target_os = "macos")]
pub use macos::create_status_item;
