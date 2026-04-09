#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use drift_core::{ColorMode, ColorPreset, NowPlayingSource, Settings};
use serde::{Deserialize, Serialize};

pub const SUPPRESS_MENU_BAR_TRAY_ENV: &str = "DRIFT_SUPPRESS_MENU_BAR_TRAY";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WallpaperLayout {
    #[default]
    PerMonitor,
    SpanDisplays,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MonitorMode {
    #[default]
    Linked,
    Independent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorConfig {
    pub monitor_id: String,
    pub name_hint: String,
    #[serde(alias = "fluxSettings")]
    pub drift_settings: Settings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredMonitor {
    pub id: String,
    pub name_hint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppConfig {
    pub enabled: bool,
    /// When set, wallpaper uses roughly half the normal refresh rate and coarser fluid steps.
    pub battery_saver: bool,
    pub launch_at_login: bool,
    pub monitor_mode: MonitorMode,
    pub wallpaper_layout: WallpaperLayout,
    /// User-chosen UI accent (swatches / color panel). Not overwritten by now-playing updates.
    pub ui_accent_override: Option<String>,
    /// Last accent derived from now-playing artwork (settings UI when user override is unset).
    pub now_playing_accent_hex: Option<String>,
    /// Last three-stop palette from now-playing artwork (settings palette strip / accent wheel).
    pub now_playing_palette: Option<[[f32; 3]; 3]>,
    pub shared_profile: Settings,
    pub monitors: BTreeMap<String, MonitorConfig>,
    pub selected_monitor_id: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            battery_saver: false,
            launch_at_login: false,
            monitor_mode: MonitorMode::Linked,
            wallpaper_layout: WallpaperLayout::PerMonitor,
            ui_accent_override: None,
            now_playing_accent_hex: None,
            now_playing_palette: None,
            shared_profile: Settings::default(),
            monitors: BTreeMap::new(),
            selected_monitor_id: None,
        }
    }
}

impl AppConfig {
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

    pub fn load() -> Self {
        Self::try_load().unwrap_or_default()
    }

    pub fn try_load() -> Result<Self> {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<Self>(&content) {
                Ok(mut config) => {
                    config.normalize();
                    Ok(config)
                }
                Err(_) => Ok(Self::default()),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error).with_context(|| format!("read config {path:?}")),
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("create config dir {dir:?}"))?;
        }

        let json = serde_json::to_vec_pretty(self).context("serialise config")?;
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, json).with_context(|| format!("write config {tmp_path:?}"))?;
        std::fs::rename(&tmp_path, &path)
            .with_context(|| format!("replace config {path:?} from {tmp_path:?}"))?;
        Ok(())
    }

    pub fn normalize(&mut self) {
        if self.shared_profile.noise_channels.is_empty() {
            self.shared_profile = Settings::default();
        }

        if self
            .ui_accent_override
            .as_deref()
            .is_some_and(|hex| !is_valid_hex_color(hex))
        {
            self.ui_accent_override = None;
        }

        if self
            .now_playing_accent_hex
            .as_deref()
            .is_some_and(|hex| !is_valid_hex_color(hex))
        {
            self.now_playing_accent_hex = None;
        }

        if self.monitors.is_empty() {
            self.wallpaper_layout = WallpaperLayout::PerMonitor;
        }

        if self.monitors.is_empty() {
            self.selected_monitor_id = None;
        } else if self
            .selected_monitor_id
            .as_ref()
            .is_none_or(|id| !self.monitors.contains_key(id))
        {
            self.selected_monitor_id = self.monitors.keys().next().cloned();
        }
    }

    pub fn ensure_monitors(&mut self, monitors: &[DiscoveredMonitor]) -> bool {
        let mut changed = false;
        for monitor in monitors {
            let entry = self
                .monitors
                .entry(monitor.id.clone())
                .or_insert_with(|| MonitorConfig {
                    monitor_id: monitor.id.clone(),
                    name_hint: monitor.name_hint.clone(),
                    drift_settings: self.shared_profile.clone(),
                });
            if entry.name_hint != monitor.name_hint {
                entry.name_hint = monitor.name_hint.clone();
                changed = true;
            }
        }

        if self.selected_monitor_id.is_none() {
            self.selected_monitor_id = monitors.first().map(|monitor| monitor.id.clone());
            changed = changed || self.selected_monitor_id.is_some();
        }

        changed
    }

    pub fn selected_monitor_id(&self) -> Option<&str> {
        self.selected_monitor_id.as_deref()
    }

    pub fn select_monitor(&mut self, monitor_id: impl Into<String>) {
        self.selected_monitor_id = Some(monitor_id.into());
    }

    pub fn selected_monitor_name(&self) -> String {
        self.selected_monitor_id()
            .and_then(|id| self.monitors.get(id))
            .map(|monitor| monitor.name_hint.clone())
            .unwrap_or_else(|| "Shared profile".to_string())
    }

    pub fn active_profile(&self) -> &Settings {
        if self.monitor_mode == MonitorMode::Linked {
            &self.shared_profile
        } else {
            self.selected_monitor_id()
                .and_then(|id| self.monitors.get(id))
                .map(|monitor| &monitor.drift_settings)
                .unwrap_or(&self.shared_profile)
        }
    }

    pub fn wallpaper_profile(&self) -> &Settings {
        match self.monitor_mode {
            MonitorMode::Linked => &self.shared_profile,
            MonitorMode::Independent => self
                .selected_monitor_id()
                .and_then(|id| self.monitors.get(id))
                .map(|monitor| &monitor.drift_settings)
                .unwrap_or(&self.shared_profile),
        }
    }

    /// Which desktop now-playing source the background worker should poll so every display that
    /// uses album art still updates — not only the monitor selected in settings.
    pub fn now_playing_poll_source(&self) -> Option<NowPlayingSource> {
        if self.wallpaper_layout == WallpaperLayout::SpanDisplays && !self.monitors.is_empty() {
            return self.wallpaper_profile().color_mode.now_playing_source();
        }

        match self.monitor_mode {
            MonitorMode::Linked => self.shared_profile.color_mode.now_playing_source(),
            MonitorMode::Independent => Self::merge_now_playing_sources(
                self.monitors
                    .values()
                    .filter_map(|m| m.drift_settings.color_mode.now_playing_source()),
            ),
        }
    }

    fn merge_now_playing_sources(
        sources: impl Iterator<Item = NowPlayingSource>,
    ) -> Option<NowPlayingSource> {
        let mut has_auto = false;
        let mut has_spotify = false;
        let mut has_apple = false;
        for s in sources {
            match s {
                NowPlayingSource::Automatic => has_auto = true,
                NowPlayingSource::Spotify => has_spotify = true,
                NowPlayingSource::AppleMusic => has_apple = true,
            }
        }
        if has_auto {
            return Some(NowPlayingSource::Automatic);
        }
        if has_spotify && has_apple {
            return Some(NowPlayingSource::Automatic);
        }
        if has_spotify {
            return Some(NowPlayingSource::Spotify);
        }
        if has_apple {
            return Some(NowPlayingSource::AppleMusic);
        }
        None
    }

    pub fn active_profile_mut(&mut self) -> &mut Settings {
        if self.monitor_mode == MonitorMode::Linked {
            &mut self.shared_profile
        } else {
            let id = self
                .selected_monitor_id
                .clone()
                .or_else(|| self.monitors.keys().next().cloned())
                .unwrap_or_else(|| {
                    let monitor_id = "display-1".to_string();
                    self.monitors.insert(
                        monitor_id.clone(),
                        MonitorConfig {
                            monitor_id: monitor_id.clone(),
                            name_hint: "Display 1".to_string(),
                            drift_settings: self.shared_profile.clone(),
                        },
                    );
                    self.selected_monitor_id = Some(monitor_id.clone());
                    monitor_id
                });
            self.selected_monitor_id = Some(id.clone());
            &mut self
                .monitors
                .entry(id.clone())
                .or_insert_with(|| MonitorConfig {
                    monitor_id: id.clone(),
                    name_hint: id.clone(),
                    drift_settings: self.shared_profile.clone(),
                })
                .drift_settings
        }
    }

    pub fn settings_for_monitor(&self, monitor_id: &str) -> Settings {
        match self.monitor_mode {
            MonitorMode::Linked => self.shared_profile.clone(),
            MonitorMode::Independent => self
                .monitors
                .get(monitor_id)
                .map(|monitor| monitor.drift_settings.clone())
                .unwrap_or_else(|| self.shared_profile.clone()),
        }
    }

    pub fn set_monitor_mode(&mut self, mode: MonitorMode) {
        self.monitor_mode = mode;
        if mode == MonitorMode::Linked {
            self.sync_linked_monitors();
        }
    }

    pub fn set_wallpaper_layout(&mut self, layout: WallpaperLayout) {
        self.wallpaper_layout = layout;
    }

    pub fn apply_preset_to_active(&mut self, preset: ColorPreset) {
        self.active_profile_mut().color_mode = ColorMode::Preset(preset);
    }

    pub fn apply_color_mode_to_all(&mut self, color_mode: ColorMode) {
        self.shared_profile.color_mode = color_mode.clone();
        for monitor in self.monitors.values_mut() {
            monitor.drift_settings.color_mode = color_mode.clone();
        }
    }

    pub fn sync_linked_monitors(&mut self) {
        if self.monitor_mode == MonitorMode::Linked {
            for monitor in self.monitors.values_mut() {
                monitor.drift_settings = self.shared_profile.clone();
            }
        }
    }
}

fn is_valid_hex_color(value: &str) -> bool {
    let hex = value.trim().trim_start_matches('#');
    hex.len() == 6 && u32::from_str_radix(hex, 16).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(id: &str, name: &str) -> DiscoveredMonitor {
        DiscoveredMonitor {
            id: id.to_string(),
            name_hint: name.to_string(),
        }
    }

    #[test]
    fn roundtrip_linked_config() {
        let mut config = AppConfig::default();
        config.ensure_monitors(&[monitor("one", "Display 1")]);
        let json = serde_json::to_string(&config).unwrap();
        let decoded: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.monitor_mode, MonitorMode::Linked);
        assert_eq!(decoded.shared_profile, config.shared_profile);
    }

    #[test]
    fn roundtrip_independent_monitors() {
        let mut config = AppConfig {
            monitor_mode: MonitorMode::Independent,
            ..AppConfig::default()
        };
        config.ensure_monitors(&[monitor("one", "Display 1"), monitor("two", "Display 2")]);
        config.select_monitor("two");
        config.active_profile_mut().line_width = 12.0;
        let json = serde_json::to_string(&config).unwrap();
        let decoded: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.monitor_mode, MonitorMode::Independent);
        assert_eq!(decoded.monitors["two"].drift_settings.line_width, 12.0);
    }

    #[test]
    fn active_profile_selection_persists_in_independent_mode() {
        let mut config = AppConfig {
            monitor_mode: MonitorMode::Independent,
            ..AppConfig::default()
        };
        config.ensure_monitors(&[monitor("one", "Display 1"), monitor("two", "Display 2")]);
        config.selected_monitor_id = None;
        let _ = config.active_profile_mut();
        assert!(config.selected_monitor_id.is_some());
    }

    #[test]
    fn missing_config_defaults_to_drift() {
        let config = AppConfig::default();
        assert_eq!(config.shared_profile, Settings::default());
    }

    #[test]
    fn color_mode_updates_apply_to_all_monitors() {
        let mut config = AppConfig {
            monitor_mode: MonitorMode::Independent,
            ..AppConfig::default()
        };
        config.ensure_monitors(&[monitor("one", "Display 1"), monitor("two", "Display 2")]);

        config.apply_color_mode_to_all(ColorMode::Preset(ColorPreset::Plasma));

        assert_eq!(
            config.shared_profile.color_mode,
            ColorMode::Preset(ColorPreset::Plasma)
        );
        assert_eq!(
            config.monitors["one"].drift_settings.color_mode,
            ColorMode::Preset(ColorPreset::Plasma)
        );
        assert_eq!(
            config.monitors["two"].drift_settings.color_mode,
            ColorMode::Preset(ColorPreset::Plasma)
        );
    }

    #[test]
    fn wallpaper_layout_defaults_to_per_monitor() {
        let config = AppConfig::default();
        assert_eq!(config.wallpaper_layout, WallpaperLayout::PerMonitor);
    }

    #[test]
    fn legacy_shape_falls_back_to_defaults() {
        let legacy = r#"{"enabled":true,"params":{"speed":1.0}}"#;
        let decoded = serde_json::from_str::<AppConfig>(legacy).unwrap();
        assert_eq!(decoded, AppConfig::default());
    }

    #[test]
    fn now_playing_poll_source_independent_uses_any_monitor_not_only_selected() {
        let m1 = MonitorConfig {
            monitor_id: "a".into(),
            name_hint: "A".into(),
            drift_settings: drift_core::Settings {
                color_mode: ColorMode::NowPlaying(NowPlayingSource::Spotify),
                ..Default::default()
            },
        };
        let m2 = MonitorConfig {
            monitor_id: "b".into(),
            name_hint: "B".into(),
            drift_settings: drift_core::Settings::default(),
        };
        let mut cfg = AppConfig {
            monitor_mode: MonitorMode::Independent,
            wallpaper_layout: WallpaperLayout::PerMonitor,
            selected_monitor_id: Some("b".into()),
            ..AppConfig::default()
        };
        cfg.monitors.insert("a".into(), m1);
        cfg.monitors.insert("b".into(), m2);

        assert_eq!(
            cfg.now_playing_poll_source(),
            Some(NowPlayingSource::Spotify)
        );
    }
}
