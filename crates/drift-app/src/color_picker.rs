#[cfg(target_os = "macos")]
mod macos {
    use std::cell::RefCell;
    use std::sync::{Arc, Mutex, OnceLock};

    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::sel;
    use objc2::{define_class, MainThreadOnly};
    use objc2_app_kit::{NSColor, NSColorPanel, NSColorPanelMode, NSColorSpace};
    use objc2_foundation::{MainThreadMarker, NSObject, NSObjectProtocol};

    use crate::config::AppConfig;

    type SharedConfig = Arc<Mutex<AppConfig>>;

    static COLOR_CONFIG: OnceLock<SharedConfig> = OnceLock::new();

    thread_local! {
        static COLOR_TARGET: RefCell<Option<Retained<DriftColorPanelTarget>>> = const { RefCell::new(None) };
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "DriftColorPanelTarget"]
        struct DriftColorPanelTarget;

        impl DriftColorPanelTarget {
            #[unsafe(method(driftColorChanged:))]
            fn color_changed(&self, _sender: Option<&AnyObject>) {
                let Some(config) = COLOR_CONFIG.get() else {
                    return;
                };

                let mtm = unsafe { MainThreadMarker::new_unchecked() };
                let panel = NSColorPanel::sharedColorPanel(mtm);
                let color = panel.color();
                let Some(hex) = color_to_hex(&color) else {
                    return;
                };

                if let Ok(mut guard) = config.lock() {
                    guard.ui_accent_override = Some(hex);
                    if let Err(error) = guard.save() {
                        log::warn!("save accent override: {error}");
                    }
                }
            }
        }

        unsafe impl NSObjectProtocol for DriftColorPanelTarget {}
    );

    impl DriftColorPanelTarget {
        fn new(mtm: MainThreadMarker) -> Retained<Self> {
            unsafe {
                let this = Self::alloc(mtm).set_ivars(());
                msg_send![super(this), init]
            }
        }
    }

    fn target_ptr(mtm: MainThreadMarker) -> *mut DriftColorPanelTarget {
        COLOR_TARGET.with(|cell| {
            let mut slot = cell.borrow_mut();
            if slot.is_none() {
                *slot = Some(DriftColorPanelTarget::new(mtm));
            }
            Retained::as_ptr(slot.as_ref().expect("color panel target"))
                as *mut DriftColorPanelTarget
        })
    }

    pub fn open_accent_color_panel(config: SharedConfig, initial_hex: &str) {
        let _ = COLOR_CONFIG.set(config);

        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };

        let Some(color) = hex_to_color(initial_hex) else {
            return;
        };

        let panel = NSColorPanel::sharedColorPanel(mtm);
        let target_ptr = target_ptr(mtm);

        unsafe {
            panel.setTarget(Some(&*target_ptr));
            panel.setAction(Some(sel!(driftColorChanged:)));
        }
        panel.setContinuous(true);
        panel.setShowsAlpha(false);
        NSColorPanel::setPickerMode(NSColorPanelMode::Wheel, mtm);
        NSColorPanel::setPickerMask(objc2_app_kit::NSColorPanelOptions::AllModesMask, mtm);
        panel.setColor(&color);
        panel.orderFrontRegardless();
    }

    fn hex_to_color(hex: &str) -> Option<Retained<NSColor>> {
        let hex = hex.trim().trim_start_matches('#');
        if hex.len() != 6 {
            return None;
        }
        let value = u32::from_str_radix(hex, 16).ok()?;
        let red = ((value >> 16) & 0xff) as f64 / 255.0;
        let green = ((value >> 8) & 0xff) as f64 / 255.0;
        let blue = (value & 0xff) as f64 / 255.0;
        Some(NSColor::colorWithSRGBRed_green_blue_alpha(
            red, green, blue, 1.0,
        ))
    }

    fn color_to_hex(color: &NSColor) -> Option<String> {
        let space = NSColorSpace::sRGBColorSpace();
        let converted = color.colorUsingColorSpace(&space);
        let source = converted.as_deref().unwrap_or(color);
        let red = (source.redComponent().clamp(0.0, 1.0) * 255.0).round() as u8;
        let green = (source.greenComponent().clamp(0.0, 1.0) * 255.0).round() as u8;
        let blue = (source.blueComponent().clamp(0.0, 1.0) * 255.0).round() as u8;
        Some(format!("#{red:02x}{green:02x}{blue:02x}"))
    }
}

#[cfg(target_os = "macos")]
pub use macos::open_accent_color_panel;

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
pub fn open_accent_color_panel(
    _: std::sync::Arc<std::sync::Mutex<crate::config::AppConfig>>,
    _: &str,
) {
}
