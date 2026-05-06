#[cfg(target_os = "macos")]
mod macos {
    use std::cell::RefCell;
    use std::sync::{Arc, OnceLock, RwLock};

    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::sel;
    use objc2::{define_class, MainThreadOnly};
    use objc2_app_kit::{
        NSApplication, NSControlStateValueOff, NSControlStateValueOn, NSMenu, NSMenuItem,
        NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
    };
    use objc2_foundation::{MainThreadMarker, NSObject, NSObjectProtocol, NSString};

    use crate::config::{AppConfig, SUPPRESS_MENU_BAR_TRAY_ENV};
    use crate::launch_agent;

    pub type SharedConfig = Arc<RwLock<AppConfig>>;

    static MENU_CONFIG: OnceLock<SharedConfig> = OnceLock::new();

    thread_local! {
        static MENU_TARGET: RefCell<Option<Retained<DriftMenuTarget>>> = const { RefCell::new(None) };
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
                let _ = std::process::Command::new(exe)
                    .arg("--settings")
                    .env(SUPPRESS_MENU_BAR_TRAY_ENV, "1")
                    .spawn();
            }

            #[unsafe(method(driftToggleEnabled:))]
            fn toggle_enabled(&self, _sender: Option<&AnyObject>) {
                let Some(cfg) = MENU_CONFIG.get() else {
                    return;
                };
                if let Ok(mut guard) = cfg.write() {
                    guard.enabled = !guard.enabled;
                    let _ = guard.save();
                }
            }

            #[unsafe(method(driftToggleLinked:))]
            fn toggle_linked(&self, _sender: Option<&AnyObject>) {
                let Some(cfg) = MENU_CONFIG.get() else {
                    return;
                };
                if let Ok(mut guard) = cfg.write() {
                    let next = match guard.monitor_mode {
                        crate::config::MonitorMode::Linked => crate::config::MonitorMode::Independent,
                        crate::config::MonitorMode::Independent => crate::config::MonitorMode::Linked,
                    };
                    guard.set_monitor_mode(next);
                    let _ = guard.save();
                }
            }

            #[unsafe(method(driftToggleLogin:))]
            fn toggle_login(&self, _sender: Option<&AnyObject>) {
                let Some(cfg) = MENU_CONFIG.get() else {
                    return;
                };
                if let Ok(mut guard) = cfg.write() {
                    let next = !guard.launch_at_login;
                    guard.launch_at_login = next;
                    let result = if next {
                        launch_agent::install()
                    } else {
                        launch_agent::uninstall()
                    };
                    if result.is_err() {
                        guard.launch_at_login = !next;
                    }
                    let _ = guard.save();
                }
            }

            #[unsafe(method(driftQuit:))]
            fn quit(&self, _sender: Option<&AnyObject>) {
                unsafe {
                    NSApplication::sharedApplication(MainThreadMarker::new_unchecked()).terminate(None);
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
            Retained::as_ptr(slot.as_ref().expect("menu target")) as *mut DriftMenuTarget
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
            button.setTitle(&NSString::from_str("~"));
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
        let state = config.read().map(|guard| guard.clone()).unwrap_or_default();

        let settings_item =
            make_action_item(mtm, "Open Settings…", sel!(driftOpenSettings:), target_ptr);
        menu.addItem(&settings_item);

        let enabled_item =
            make_action_item(mtm, "Wallpaper live", sel!(driftToggleEnabled:), target_ptr);
        enabled_item.setState(if state.enabled {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        menu.addItem(&enabled_item);

        let linked_item = make_action_item(
            mtm,
            "Link monitor settings",
            sel!(driftToggleLinked:),
            target_ptr,
        );
        linked_item.setState(
            if state.monitor_mode == crate::config::MonitorMode::Linked {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            },
        );
        menu.addItem(&linked_item);

        let login_item =
            make_action_item(mtm, "Open at login", sel!(driftToggleLogin:), target_ptr);
        login_item.setState(if state.launch_at_login {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        menu.addItem(&login_item);

        menu.addItem(&NSMenuItem::separatorItem(mtm));

        let quit_item = make_action_item(mtm, "Quit", sel!(driftQuit:), target_ptr);
        menu.addItem(&quit_item);

        menu
    }

    fn make_action_item(
        mtm: MainThreadMarker,
        title: &str,
        action: objc2::runtime::Sel,
        target_ptr: *mut DriftMenuTarget,
    ) -> Retained<NSMenuItem> {
        let item = NSMenuItem::new(mtm);
        item.setTitle(&NSString::from_str(title));
        unsafe { item.setAction(Some(action)) };
        unsafe { item.setTarget(Some(&*target_ptr)) };
        item
    }
}

#[cfg(target_os = "macos")]
pub use macos::create_status_item;
