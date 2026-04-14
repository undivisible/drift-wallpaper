//! macOS color picker via NSColorPanel.

use std::sync::{Arc, Mutex};

use crate::config::AppConfig;
use crate::platform::NativeColorPicker;

pub struct MacosColorPicker;

impl NativeColorPicker for MacosColorPicker {
    fn pick_color(initial: Option<[u8; 3]>) -> anyhow::Result<Option<[u8; 3]>> {
        let hex = initial.map(|[r, g, b]| format!("#{r:02x}{g:02x}{b:02x}"));
        let hex = hex.as_deref().unwrap_or("#000000");
        crate::color_picker::open_accent_color_panel(
            Arc::new(Mutex::new(AppConfig::default())),
            hex,
        );
        Ok(None)
    }
}
