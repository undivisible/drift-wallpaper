//! Application configuration: persisted to `~/Library/Application Support/drift-wallpaper/config.json`
//! on macOS, or a local `drift-config.json` fallback on other platforms.
// Items in this module are used only from the macOS-specific code path.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use anyhow::{Context, Result};
use drift_core::{
    color::{ColorPalette, Preset},
    simulation::DriftParams,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level application config persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Whether the live wallpaper is currently enabled.
    pub enabled: bool,
    /// Simulation parameters (speed, scale, colours).
    pub params: DriftParams,
    /// Launch at system login.
    pub launch_at_login: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            params: DriftParams::from_palette(&ColorPalette::preset(Preset::Midnight), 1.0),
            launch_at_login: false,
        }
    }
}

impl AppConfig {
    /// Return the path to the config file.
    pub fn config_path() -> PathBuf {
        #[cfg(target_os = "macos")]
        {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_owned());
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("drift-wallpaper")
                .join("config.json")
        }
        #[cfg(not(target_os = "macos"))]
        {
            PathBuf::from("drift-config.json")
        }
    }

    /// Load from disk, returning the default config if the file does not exist.
    pub fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
                log::warn!("Failed to parse config at {path:?}: {e}. Using defaults.");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Persist the config to disk.  Creates parent directories if needed.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("create config dir {dir:?}"))?;
        }
        let json = serde_json::to_string_pretty(self).context("serialise config")?;
        std::fs::write(&path, json).with_context(|| format!("write config {path:?}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let cfg = AppConfig::default();
        assert!(cfg.params.speed > 0.0);
    }

    #[test]
    fn config_roundtrip_json() {
        let cfg = AppConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let cfg2: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg.enabled, cfg2.enabled);
        assert_eq!(cfg.launch_at_login, cfg2.launch_at_login);
    }
}
