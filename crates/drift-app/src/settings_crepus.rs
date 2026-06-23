use std::collections::HashMap;

/// Embedded at compile time so release/CI binaries do not read `views/` from disk.
pub const SETTINGS_UI_CREPUS: &str = include_str!("../views/settings_ui.crepus");

pub fn settings_ui_virtual_files() -> HashMap<String, String> {
    let mut files = HashMap::new();
    files.insert(
        "settings_ui.crepus".to_string(),
        SETTINGS_UI_CREPUS.to_string(),
    );
    files
}
