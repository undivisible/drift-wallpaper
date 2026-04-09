//! macOS menubar status item implemented with objc2.
//!
//! Provides a 🌊 icon in the system menu bar with the following options:
//! - Open Settings (separate window; does not add a second tray icon)
//! - Enable / Disable wallpaper (persists to config)
//! - Select colour preset (Ocean, Sunset, Forest, Lava, Midnight, Monochrome)
//! - Extract colors from an image via NSOpenPanel
//! - Start at Login toggle
//! - Quit

#[cfg(target_os = "macos")]
mod macos {
    use std::cell::RefCell;
    use std::sync::{Arc, Mutex, OnceLock};

    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::sel;
    use objc2::{define_class, MainThreadOnly};
    use objc2_app_kit::{
        NSAlert, NSAlertStyle, NSApplication, NSControlStateValueOff, NSControlStateValueOn,
        NSMenu, NSMenuItem, NSModalResponseOK, NSOpenPanel, NSStatusBar, NSStatusItem,
        NSVariableStatusItemLength,
    };
    use objc2_foundation::{MainThreadMarker, NSObject, NSObjectProtocol, NSString};

    use crate::cli;
    use crate::config::{AppConfig, SUPPRESS_MENU_BAR_TRAY_ENV};
    use crate::launch_agent;

    pub type SharedConfig = Arc<Mutex<AppConfig>>;

    static MENU_CONFIG: OnceLock<SharedConfig> = OnceLock::new();

    // Retains the menu target for the process lifetime (AppKit does not retain `setTarget:`).
    // Main-thread `thread_local` avoids leaking `Retained` and keeps the type off `Sync` statics.
    thread_local! {
        static MENU_TARGET: RefCell<Option<Retained<DriftMenuTarget>>> = const { RefCell::new(None) };
    }

    fn show_error_alert(mtm: MainThreadMarker, title: &str, detail: &str) {
        let alert = NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str(title));
        alert.setInformativeText(&NSString::from_str(detail));
        alert.setAlertStyle(NSAlertStyle::Warning);
        let _ = alert.runModal();
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "DriftMenuTarget"]
        struct DriftMenuTarget;

        impl DriftMenuTarget {
            #[unsafe(method(driftOpenSettings:))]
            fn open_settings(&self, _sender: Option<&AnyObject>) {
                let Ok(exe) = std::env::current_exe() else {
                    return;
                };
                if let Err(err) = std::process::Command::new(exe)
                    .arg("--settings")
                    .env(SUPPRESS_MENU_BAR_TRAY_ENV, "1")
                    .spawn()
                {
                    log::error!("Failed to open Drift settings: {err}");
                }
            }

            #[unsafe(method(driftToggleEnabled:))]
            fn toggle_enabled(&self, _sender: Option<&AnyObject>) {
                let Some(cfg) = MENU_CONFIG.get() else {
                    return;
                };
                if let Ok(mut g) = cfg.lock() {
                    g.enabled = !g.enabled;
                    if let Err(err) = g.save() {
                        log::error!("Failed to save config after toggling wallpaper: {err}");
                    }
                }
            }

            #[unsafe(method(driftApplyPreset:))]
            fn apply_preset(&self, sender: Option<&AnyObject>) {
                let Some(cfg) = MENU_CONFIG.get() else {
                    return;
                };
                let Some(sender) = sender else {
                    return;
                };
                let Some(item) = sender.downcast_ref::<NSMenuItem>() else {
                    return;
                };
                let idx = item.tag();
                if idx < 0 {
                    return;
                }
                let Some(preset) = drift_core::color::Preset::all().get(idx as usize) else {
                    return;
                };
                if let Ok(mut g) = cfg.lock() {
                    let name = preset.label().to_ascii_lowercase();
                    if let Err(err) =
                        cli::apply_named_preset(&mut g, &name).and_then(|_| g.save())
                    {
                        log::error!("Failed to apply preset {name}: {err}");
                    }
                }
            }

            #[unsafe(method(driftPickImage:))]
            fn pick_image(&self, _sender: Option<&AnyObject>) {
                let Some(cfg) = MENU_CONFIG.get() else {
                    return;
                };
                let mtm = MainThreadMarker::new().expect("menu actions must run on the main thread");
                let panel = NSOpenPanel::openPanel(mtm);
                panel.setCanChooseFiles(true);
                panel.setCanChooseDirectories(false);
                panel.setAllowsMultipleSelection(false);
                if panel.runModal() != NSModalResponseOK {
                    return;
                }
                let urls = panel.URLs();
                if urls.count() == 0 {
                    return;
                }
                let Some(url) = urls.firstObject() else {
                    return;
                };
                let Some(path_ns) = url.path() else {
                    return;
                };
                let path = path_ns.to_string();
                if let Ok(mut g) = cfg.lock() {
                    if let Err(err) =
                        cli::apply_image_palette(&mut g, std::path::Path::new(&path))
                            .and_then(|_| g.save())
                    {
                        log::error!("Failed to apply image palette: {err}");
                    }
                }
            }

            #[unsafe(method(driftToggleLogin:))]
            fn toggle_login(&self, _sender: Option<&AnyObject>) {
                let Some(cfg) = MENU_CONFIG.get() else {
                    return;
                };
                if let Ok(mut g) = cfg.lock() {
                    let next = !g.launch_at_login;
                    g.launch_at_login = next;
                    let agent_res = if next {
                        launch_agent::install()
                    } else {
                        launch_agent::uninstall()
                    };
                    if let Err(err) = agent_res.and_then(|_| g.save()) {
                        log::error!("Failed to update launch-at-login: {err}");
                        g.launch_at_login = !next;
                        let mtm =
                            MainThreadMarker::new().expect("menu actions must run on the main thread");
                        show_error_alert(
                            mtm,
                            "Couldn’t update Start at Login",
                            &format!(
                                "{err}\n\nYour preference was reverted; try again or check Console for details."
                            ),
                        );
                    }
                }
            }
        }

        unsafe impl NSObjectProtocol for DriftMenuTarget {}
    );

    impl DriftMenuTarget {
        fn new(mtm: MainThreadMarker) -> Retained<Self> {
            unsafe {
                let this = Self::alloc(mtm).set_ivars(());
                msg_send![super(this), init]
            }
        }
    }

    fn menu_target_ptr(mtm: MainThreadMarker) -> *mut DriftMenuTarget {
        MENU_TARGET.with(|cell| {
            let mut slot = cell.borrow_mut();
            if slot.is_none() {
                *slot = Some(DriftMenuTarget::new(mtm));
            }
            Retained::as_ptr(slot.as_ref().expect("just set")) as *mut DriftMenuTarget
        })
    }

    pub fn create_status_item(
        mtm: MainThreadMarker,
        config: SharedConfig,
    ) -> Retained<NSStatusItem> {
        let _ = MENU_CONFIG.set(Arc::clone(&config));
        let target_ptr = menu_target_ptr(mtm);

        let status_bar = NSStatusBar::systemStatusBar();
        let status_item = status_bar.statusItemWithLength(NSVariableStatusItemLength);

        if let Some(button) = status_item.button(mtm) {
            let title = NSString::from_str("🌊");
            button.setTitle(&title);
        }

        let menu = build_menu(mtm, &config, target_ptr);
        status_item.setMenu(Some(&menu));

        status_item
    }

    fn build_menu(
        mtm: MainThreadMarker,
        config: &SharedConfig,
        target_ptr: *mut DriftMenuTarget,
    ) -> Retained<NSMenu> {
        let menu = NSMenu::new(mtm);

        let settings_item =
            make_action_item(mtm, "Open Settings…", sel!(driftOpenSettings:), target_ptr);
        menu.addItem(&settings_item);

        separator(&menu, mtm);

        let enabled = config.lock().unwrap().enabled;

        let toggle_item =
            make_action_item(mtm, "Wallpaper live", sel!(driftToggleEnabled:), target_ptr);
        toggle_item.setState(if enabled {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        menu.addItem(&toggle_item);

        separator(&menu, mtm);

        let presets_item = make_item(mtm, "Colour Preset", None);
        let presets_menu = NSMenu::new(mtm);
        for (idx, preset) in drift_core::color::Preset::all().iter().enumerate() {
            let item = make_action_item(mtm, preset.label(), sel!(driftApplyPreset:), target_ptr);
            item.setTag(idx as isize);
            presets_menu.addItem(&item);
        }
        presets_item.setSubmenu(Some(&presets_menu));
        menu.addItem(&presets_item);

        let upload_item = make_action_item(
            mtm,
            "Extract Colors from Image…",
            sel!(driftPickImage:),
            target_ptr,
        );
        menu.addItem(&upload_item);

        separator(&menu, mtm);

        let login = config.lock().unwrap().launch_at_login;
        let login_item =
            make_action_item(mtm, "Start at Login", sel!(driftToggleLogin:), target_ptr);
        login_item.setState(if login {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        menu.addItem(&login_item);

        separator(&menu, mtm);

        let quit_item = make_action_item(
            mtm,
            "Quit Drift Wallpaper",
            sel!(terminate:),
            std::ptr::null_mut(),
        );
        let app = NSApplication::sharedApplication(mtm);
        unsafe {
            quit_item.setTarget(Some(app.as_ref()));
        }
        menu.addItem(&quit_item);

        menu
    }

    fn make_item(
        mtm: MainThreadMarker,
        title: &str,
        action: Option<objc2::runtime::Sel>,
    ) -> Retained<NSMenuItem> {
        let title_ns = NSString::from_str(title);
        let key = NSString::from_str("");
        unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &title_ns,
                action,
                &key,
            )
        }
    }

    fn make_action_item(
        mtm: MainThreadMarker,
        title: &str,
        action: objc2::runtime::Sel,
        target_ptr: *mut DriftMenuTarget,
    ) -> Retained<NSMenuItem> {
        let item = make_item(mtm, title, Some(action));
        unsafe {
            if target_ptr.is_null() {
                item.setTarget(None);
            } else {
                item.setTarget(Some(
                    target_ptr
                        .cast::<AnyObject>()
                        .as_ref()
                        .expect("menu target"),
                ));
            }
        }
        item
    }

    fn separator(menu: &NSMenu, mtm: MainThreadMarker) {
        let sep = NSMenuItem::separatorItem(mtm);
        menu.addItem(&sep);
    }
}

#[cfg(target_os = "macos")]
pub use macos::create_status_item;
