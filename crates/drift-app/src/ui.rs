use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result};
use crepuscularity_runtime::{parse_component_file, ComponentFile, TemplateContext};

use crate::crepus_interactive::CrepusMouseDispatch;
use crate::crepus_settings_render::render_nodes_interactive;
use drift_core::settings::{COLOR_SCHEME_PLASMA, COLOR_SCHEME_POOLSIDE};
use drift_core::{ColorMode, ColorPreset, Mode, NowPlayingSource, PressureMode, Settings};
use gpui::{
    actions, bounds, div, point, px, rgb, size, App as GpuiApp, AppContext, Application, Context,
    IntoElement, KeyBinding, MouseUpEvent, ParentElement, PathPromptOptions, Render, Styled,
    WeakEntity, Window, WindowBounds, WindowOptions,
};

use crate::{
    cli,
    config::{AppConfig, MonitorMode, WallpaperLayout},
};

#[cfg(target_os = "macos")]
use crate::menubar;

#[cfg(target_os = "macos")]
use crate::config::SUPPRESS_MENU_BAR_TRAY_ENV;

actions!(drift_app_actions, [Quit]);

pub fn run_ui(initial_config: AppConfig) -> Result<()> {
    let shared = Arc::new(RwLock::new(initial_config));
    let shared_window = Arc::clone(&shared);
    #[cfg(target_os = "macos")]
    let show_menu_bar_tray = std::env::var_os(SUPPRESS_MENU_BAR_TRAY_ENV).is_none();

    Application::new().run(move |cx: &mut GpuiApp| {
        #[cfg(target_os = "macos")]
        let _menubar_item = show_menu_bar_tray.then(|| {
            use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
            use objc2_foundation::MainThreadMarker;

            let mtm = MainThreadMarker::new().expect("GPUI must run on the main thread");
            let app = NSApplication::sharedApplication(mtm);
            app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
            menubar::create_status_item(mtm, Arc::clone(&shared))
        });

        let bounds = bounds(point(px(88.), px(72.)), size(px(600.), px(700.)));
        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            focus: true,
            show: true,
            kind: gpui::WindowKind::Normal,
            is_movable: true,
            is_resizable: true,
            is_minimizable: true,
            display_id: None,
            window_background: gpui::WindowBackgroundAppearance::Opaque,
            app_id: Some("drift-wallpaper.controls".to_string()),
            window_min_size: Some(size(px(520.), px(560.))),
            window_decorations: None,
            tabbing_identifier: None,
        };

        cx.open_window(window_options, move |_window, cx| {
            cx.new(|cx| DriftUi::new(Arc::clone(&shared_window), cx))
        })
        .expect("open gpui controls window");

        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
    });

    Ok(())
}

struct DriftUi {
    config: Arc<RwLock<AppConfig>>,
    config_refresh_started: bool,
    advanced_settings_expanded: bool,
    settings_component_file: ComponentFile,
}

impl DriftUi {
    fn new(config: Arc<RwLock<AppConfig>>, _cx: &mut Context<Self>) -> Self {
        let template_path = settings_template_path();
        let source = std::fs::read_to_string(&template_path)
            .unwrap_or_else(|e| panic!("Failed to read settings template {template_path:?}: {e}"));
        let component_file = parse_component_file(&source)
            .unwrap_or_else(|e| panic!("Failed to parse settings template: {e}"));

        Self {
            config,
            config_refresh_started: false,
            advanced_settings_expanded: false,
            settings_component_file: component_file,
        }
    }

    fn read_config(&self) -> AppConfig {
        self.config.read().map(|g| g.clone()).unwrap_or_default()
    }

    fn save_config(&self, cfg: &AppConfig) -> Result<()> {
        let mut normalized = cfg.clone();
        normalized.sync_linked_monitors();
        let mut guard = self
            .config
            .write()
            .map_err(|_| anyhow::anyhow!("config rwlock poisoned"))?;
        *guard = normalized.clone();
        guard.save()
    }

    fn modify_config(
        &mut self,
        cx: &mut Context<Self>,
        mutator: impl FnOnce(&mut AppConfig) -> Result<()>,
    ) {
        let mut cfg = self.read_config();
        match mutator(&mut cfg).and_then(|_| self.save_config(&cfg)) {
            Ok(()) => cx.notify(),
            Err(error) => log::warn!("settings update: {error}"),
        }
    }

    fn monitor_ids(cfg: &AppConfig) -> Vec<String> {
        cfg.monitors.keys().cloned().collect()
    }

    fn select_monitor_by_index(&mut self, index: usize, cx: &mut Context<Self>) {
        self.modify_config(cx, |cfg| {
            if let Some(id) = Self::monitor_ids(cfg).get(index).cloned() {
                cfg.select_monitor(id);
            }
            Ok(())
        });
    }

    fn toggle_link_mode(&mut self, cx: &mut Context<Self>) {
        self.modify_config(cx, |cfg| {
            let next = match cfg.monitor_mode {
                MonitorMode::Linked => MonitorMode::Independent,
                MonitorMode::Independent => MonitorMode::Linked,
            };
            cfg.set_monitor_mode(next);
            Ok(())
        });
    }

    fn toggle_wallpaper_layout(&mut self, cx: &mut Context<Self>) {
        self.modify_config(cx, |cfg| {
            let next = match cfg.wallpaper_layout {
                WallpaperLayout::PerMonitor => WallpaperLayout::SpanDisplays,
                WallpaperLayout::SpanDisplays => WallpaperLayout::PerMonitor,
            };
            cfg.set_wallpaper_layout(next);
            Ok(())
        });
    }

    fn update_settings(&mut self, cx: &mut Context<Self>, mutator: impl FnOnce(&mut Settings)) {
        self.modify_config(cx, |cfg| {
            mutator(cfg.active_profile_mut());
            Ok(())
        });
    }

    fn bump_u32(value: &mut u32, delta: i32, min: u32, max: u32) {
        let next = (*value as i64 + delta as i64).clamp(min as i64, max as i64);
        *value = next as u32;
    }

    fn bump_f32(value: &mut f32, delta: f32, min: f32, max: f32) {
        *value = (*value + delta).clamp(min, max);
    }

    fn cycle_mode(settings: &mut Settings) {
        settings.mode = match settings.mode {
            Mode::Normal => Mode::DebugNoise,
            Mode::DebugNoise => Mode::DebugFluid,
            Mode::DebugFluid => Mode::DebugPressure,
            Mode::DebugPressure => Mode::DebugDivergence,
            Mode::DebugDivergence => Mode::Normal,
        };
    }

    fn cycle_pressure_mode(settings: &mut Settings) {
        settings.pressure_mode = match settings.pressure_mode {
            PressureMode::Retain => PressureMode::ClearWith(0.0),
            PressureMode::ClearWith(_) => PressureMode::Retain,
        };
    }

    fn set_now_playing_source(&mut self, source: NowPlayingSource, cx: &mut Context<Self>) {
        self.modify_config(cx, |cfg| {
            cfg.ui_accent_override = None;
            cfg.now_playing_accent_hex = None;
            cfg.now_playing_palette = None;
            cfg.active_profile_mut().color_mode = ColorMode::NowPlaying(source);
            Ok(())
        });
    }

    fn random_seed() -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!("{now:x}")
    }

    fn ensure_noise(settings: &mut Settings, index: usize) {
        while settings.noise_channels.len() <= index {
            settings
                .noise_channels
                .push(Settings::default().noise_channels[0].clone());
        }
    }

    fn handle_action(
        &mut self,
        action: &str,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let action = action.trim();
        match action {
            "close_window" => {
                cx.quit();
            }
            "toggle_wallpaper_enabled" => {
                self.modify_config(cx, |cfg| {
                    cfg.enabled = !cfg.enabled;
                    Ok(())
                });
            }
            "toggle_battery_saver" => {
                self.modify_config(cx, |cfg| {
                    cfg.battery_saver = !cfg.battery_saver;
                    Ok(())
                });
            }
            "toggle_launch_at_login" => {
                #[cfg(target_os = "macos")]
                {
                    use crate::launch_agent;

                    self.modify_config(cx, |cfg| {
                        let next = !cfg.launch_at_login;
                        cfg.launch_at_login = next;
                        let result = if next {
                            launch_agent::install()
                        } else {
                            launch_agent::uninstall()
                        };
                        if let Err(error) = result {
                            cfg.launch_at_login = !next;
                            return Err(error);
                        }
                        Ok(())
                    });
                }
                #[cfg(not(target_os = "macos"))]
                let _ = cx;
            }
            "toggle_link_mode" => self.toggle_link_mode(cx),
            "toggle_wallpaper_layout" => self.toggle_wallpaper_layout(cx),
            "toggle_advanced_settings" => {
                self.advanced_settings_expanded = !self.advanced_settings_expanded;
                cx.notify();
            }
            "open_background" => {
                if let Err(error) = spawn_default_wallpaper_process() {
                    log::warn!("launch wallpaper: {error}");
                }
                cx.notify();
            }
            "open_preview" => {
                if let Err(error) = spawn_mode("--preview") {
                    log::warn!("launch preview: {error}");
                }
                cx.notify();
            }
            "pick_image_file" => self.pick_image_file(cx),
            "apply_wallpaper_image" => self.apply_current_wallpaper(cx),
            "apply_wallpaper_screenshot" => self.apply_wallpaper_screenshot(cx),
            "set_now_playing_automatic" => {
                self.set_now_playing_source(NowPlayingSource::Automatic, cx)
            }
            "set_now_playing_apple_music" => {
                self.set_now_playing_source(NowPlayingSource::AppleMusic, cx)
            }
            "set_now_playing_spotify" => self.set_now_playing_source(NowPlayingSource::Spotify, cx),
            _ if action.starts_with("pick_palette_swatch__") => {
                if let Some(index) = action
                    .strip_prefix("pick_palette_swatch__")
                    .and_then(|value| value.parse::<usize>().ok())
                {
                    self.pick_palette_swatch(index, cx);
                }
            }
            "set_preset_original" => self.modify_config(cx, |cfg| {
                cfg.ui_accent_override = None;
                cfg.now_playing_accent_hex = None;
                cfg.now_playing_palette = None;
                cfg.active_profile_mut().color_mode = ColorMode::Preset(ColorPreset::Original);
                Ok(())
            }),
            "set_preset_plasma" => self.modify_config(cx, |cfg| {
                cfg.ui_accent_override = None;
                cfg.now_playing_accent_hex = None;
                cfg.now_playing_palette = None;
                cfg.active_profile_mut().color_mode = ColorMode::Preset(ColorPreset::Plasma);
                Ok(())
            }),
            "set_preset_poolside" => self.modify_config(cx, |cfg| {
                cfg.ui_accent_override = None;
                cfg.now_playing_accent_hex = None;
                cfg.now_playing_palette = None;
                cfg.active_profile_mut().color_mode = ColorMode::Preset(ColorPreset::Poolside);
                Ok(())
            }),
            "set_preset_freedom" => self.modify_config(cx, |cfg| {
                cfg.ui_accent_override = None;
                cfg.now_playing_accent_hex = None;
                cfg.now_playing_palette = None;
                cfg.active_profile_mut().color_mode = ColorMode::Preset(ColorPreset::Freedom);
                Ok(())
            }),
            "cycle_mode" => self.update_settings(cx, Self::cycle_mode),
            "toggle_pressure_mode" => self.update_settings(cx, Self::cycle_pressure_mode),
            "pressure_clear_down" => self.update_settings(cx, |settings| {
                let clear = match settings.pressure_mode {
                    PressureMode::Retain => 0.0,
                    PressureMode::ClearWith(value) => value,
                };
                settings.pressure_mode = PressureMode::ClearWith((clear - 0.05).max(-5.0));
            }),
            "pressure_clear_up" => self.update_settings(cx, |settings| {
                let clear = match settings.pressure_mode {
                    PressureMode::Retain => 0.0,
                    PressureMode::ClearWith(value) => value,
                };
                settings.pressure_mode = PressureMode::ClearWith((clear + 0.05).min(5.0));
            }),
            "seed_randomize" => self.update_settings(cx, |settings| {
                settings.seed = Some(Self::random_seed());
            }),
            "seed_clear" => self.update_settings(cx, |settings| {
                settings.seed = None;
            }),
            _ => {
                if let Some(index) = action.strip_prefix("select_monitor__") {
                    if let Ok(index) = index.parse::<usize>() {
                        self.select_monitor_by_index(index, cx);
                    }
                    return;
                }
                if let Some(field) = action.strip_prefix("adjust__") {
                    self.apply_adjustment(field, cx);
                    return;
                }
                if let Some(field) = action.strip_prefix("noise__") {
                    self.apply_noise_adjustment(field, cx);
                    return;
                }
                log::warn!("unknown crepus action: {action}");
            }
        }
    }

    fn apply_adjustment(&mut self, payload: &str, cx: &mut Context<Self>) {
        let parts: Vec<_> = payload.split("__").collect();
        if parts.len() != 2 {
            return;
        }
        let (field, direction) = (parts[0], parts[1]);
        self.update_settings(cx, |settings| match (field, direction) {
            ("fluid_size", "down") => Self::bump_u32(&mut settings.fluid_size, -8, 32, 512),
            ("fluid_size", "up") => Self::bump_u32(&mut settings.fluid_size, 8, 32, 512),
            ("fluid_frame_rate", "down") => {
                Self::bump_f32(&mut settings.fluid_frame_rate, -5.0, 1.0, 240.0)
            }
            ("fluid_frame_rate", "up") => {
                Self::bump_f32(&mut settings.fluid_frame_rate, 5.0, 1.0, 240.0)
            }
            ("fluid_timestep", "down") => {
                Self::bump_f32(&mut settings.fluid_timestep, -0.002, 0.001, 0.2)
            }
            ("fluid_timestep", "up") => {
                Self::bump_f32(&mut settings.fluid_timestep, 0.002, 0.001, 0.2)
            }
            ("viscosity", "down") => Self::bump_f32(&mut settings.viscosity, -0.5, 0.0, 20.0),
            ("viscosity", "up") => Self::bump_f32(&mut settings.viscosity, 0.5, 0.0, 20.0),
            ("velocity_dissipation", "down") => {
                Self::bump_f32(&mut settings.velocity_dissipation, -0.01, 0.0, 1.0)
            }
            ("velocity_dissipation", "up") => {
                Self::bump_f32(&mut settings.velocity_dissipation, 0.01, 0.0, 1.0)
            }
            ("diffusion_iterations", "down") => {
                Self::bump_u32(&mut settings.diffusion_iterations, -1, 0, 100)
            }
            ("diffusion_iterations", "up") => {
                Self::bump_u32(&mut settings.diffusion_iterations, 1, 0, 100)
            }
            ("pressure_iterations", "down") => {
                Self::bump_u32(&mut settings.pressure_iterations, -1, 1, 200)
            }
            ("pressure_iterations", "up") => {
                Self::bump_u32(&mut settings.pressure_iterations, 1, 1, 200)
            }
            ("line_length", "down") => {
                Self::bump_f32(&mut settings.line_length, -25.0, 10.0, 1000.0)
            }
            ("line_length", "up") => Self::bump_f32(&mut settings.line_length, 25.0, 10.0, 1000.0),
            ("line_width", "down") => Self::bump_f32(&mut settings.line_width, -0.5, 0.5, 30.0),
            ("line_width", "up") => Self::bump_f32(&mut settings.line_width, 0.5, 0.5, 30.0),
            ("line_begin_offset", "down") => {
                Self::bump_f32(&mut settings.line_begin_offset, -0.05, 0.0, 1.0)
            }
            ("line_begin_offset", "up") => {
                Self::bump_f32(&mut settings.line_begin_offset, 0.05, 0.0, 1.0)
            }
            ("line_variance", "down") => {
                Self::bump_f32(&mut settings.line_variance, -0.05, 0.0, 2.0)
            }
            ("line_variance", "up") => Self::bump_f32(&mut settings.line_variance, 0.05, 0.0, 2.0),
            ("grid_spacing", "down") => Self::bump_u32(&mut settings.grid_spacing, -1, 2, 60),
            ("grid_spacing", "up") => Self::bump_u32(&mut settings.grid_spacing, 1, 2, 60),
            ("view_scale", "down") => Self::bump_f32(&mut settings.view_scale, -0.05, 0.1, 5.0),
            ("view_scale", "up") => Self::bump_f32(&mut settings.view_scale, 0.05, 0.1, 5.0),
            ("noise_multiplier", "down") => {
                Self::bump_f32(&mut settings.noise_multiplier, -0.05, 0.0, 5.0)
            }
            ("noise_multiplier", "up") => {
                Self::bump_f32(&mut settings.noise_multiplier, 0.05, 0.0, 5.0)
            }
            ("wallpaper_brightness", "down") => {
                Self::bump_f32(&mut settings.wallpaper_brightness, -0.05, 0.05, 2.0)
            }
            ("wallpaper_brightness", "up") => {
                Self::bump_f32(&mut settings.wallpaper_brightness, 0.05, 0.05, 2.0)
            }
            _ => {}
        });
    }

    fn apply_noise_adjustment(&mut self, payload: &str, cx: &mut Context<Self>) {
        let parts: Vec<_> = payload.split("__").collect();
        if parts.len() != 3 {
            return;
        }
        let Ok(index) = parts[0].parse::<usize>() else {
            return;
        };
        let field = parts[1];
        let direction = parts[2];
        self.update_settings(cx, move |settings| {
            Self::ensure_noise(settings, index);
            let channel = &mut settings.noise_channels[index];
            match (field, direction) {
                ("scale", "down") => Self::bump_f32(&mut channel.scale, -0.5, 0.1, 100.0),
                ("scale", "up") => Self::bump_f32(&mut channel.scale, 0.5, 0.1, 100.0),
                ("multiplier", "down") => Self::bump_f32(&mut channel.multiplier, -0.05, 0.0, 5.0),
                ("multiplier", "up") => Self::bump_f32(&mut channel.multiplier, 0.05, 0.0, 5.0),
                ("offset_increment", "down") => {
                    Self::bump_f32(&mut channel.offset_increment, -0.0005, 0.0, 0.1)
                }
                ("offset_increment", "up") => {
                    Self::bump_f32(&mut channel.offset_increment, 0.0005, 0.0, 0.1)
                }
                _ => {}
            }
        });
    }

    fn pick_image_file(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose an image".into()),
        });
        if let Ok(Ok(Some(paths))) = pollster::block_on(rx) {
            let Some(path) = paths.first() else {
                return;
            };
            self.modify_config(cx, |cfg| {
                cfg.ui_accent_override = None;
                cfg.now_playing_accent_hex = None;
                cfg.now_playing_palette = None;
                cli::apply_image_color_mode(cfg, path)
            });
        }
    }

    fn apply_current_wallpaper(&mut self, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        {
            self.modify_config(cx, |cfg| {
                cfg.ui_accent_override = None;
                cfg.now_playing_accent_hex = None;
                cfg.now_playing_palette = None;
                cli::apply_current_wallpaper_color_mode(cfg)
            });
        }
        #[cfg(not(target_os = "macos"))]
        let _ = cx;
    }

    fn apply_wallpaper_screenshot(&mut self, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        {
            self.modify_config(cx, |cfg| {
                cfg.ui_accent_override = None;
                cfg.now_playing_accent_hex = None;
                cfg.now_playing_palette = None;
                cli::apply_wallpaper_screenshot_color_mode(cfg)
            });
        }
        #[cfg(not(target_os = "macos"))]
        let _ = cx;
    }

    fn pick_palette_swatch(&mut self, index: usize, cx: &mut Context<Self>) {
        let cfg = self.read_config();
        let settings = cfg.active_profile();
        let hexes = palette_wheel_hexes(settings, cfg.now_playing_palette);
        if let Some(hex) = hexes.get(index) {
            let hex = hex.clone();
            self.modify_config(cx, |cfg| {
                cfg.ui_accent_override = Some(hex.clone());
                Ok(())
            });
            #[cfg(target_os = "macos")]
            crate::color_picker::open_accent_color_panel(Arc::clone(&self.config), &hex);
        }
    }

    fn start_config_refresh_loop(
        weak_ui: WeakEntity<DriftUi>,
        config: Arc<RwLock<AppConfig>>,
        window: &mut Window,
        cx: &mut Context<DriftUi>,
    ) {
        let path = AppConfig::config_path();
        std::mem::drop(window.spawn(cx, async move |async_cx| {
            let mut last_modified = std::fs::metadata(&path)
                .and_then(|meta| meta.modified())
                .ok();
            loop {
                async_cx
                    .background_executor()
                    .timer(Duration::from_millis(250))
                    .await;

                let modified = std::fs::metadata(&path)
                    .and_then(|meta| meta.modified())
                    .ok();
                if modified == last_modified {
                    continue;
                }
                last_modified = modified;

                if let Ok(latest) = AppConfig::try_load() {
                    if let Ok(mut current) = config.write() {
                        *current = latest;
                    }
                    let _ = weak_ui.update_in(async_cx, |_ui, window, cx| {
                        cx.notify();
                        window.refresh();
                    });
                }
            }
        }));
    }
}

impl CrepusMouseDispatch for DriftUi {
    fn dispatch_crepus_mouse_up(
        &mut self,
        action: &str,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_action(action, event, window, cx);
    }
}

impl Render for DriftUi {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.config_refresh_started {
            self.config_refresh_started = true;
            let weak = cx.weak_entity();
            let cfg_arc = Arc::clone(&self.config);
            Self::start_config_refresh_loop(weak, cfg_arc, window, cx);
        }

        let cfg = self.read_config();
        let advanced_settings_expanded = self.advanced_settings_expanded;

        let Some(root) = self.settings_component_file.components.get("SettingsRoot") else {
            return div()
                .w_full()
                .h_full()
                .p(px(16.))
                .text_color(rgb(0xf87171))
                .child("SettingsRoot component missing from generated settings")
                .into_any_element();
        };

        let tctx = build_settings_context(&cfg, advanced_settings_expanded);
        let body = render_nodes_interactive(&root.nodes, &tctx, cx);
        let viewport_h = window.bounds().size.height;
        div()
            .w_full()
            .h(viewport_h)
            .flex()
            .flex_col()
            .min_h(px(0.))
            .overflow_hidden()
            .child(
                div()
                    .w_full()
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_hidden()
                    .child(body),
            )
            .into_any_element()
    }
}

fn settings_template_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("views")
        .join("settings_ui.crepus")
}

fn build_settings_context(cfg: &AppConfig, advanced_settings_expanded: bool) -> TemplateContext {
    let settings = cfg.active_profile();
    let mut tctx = TemplateContext::new();
    tctx.base_dir = settings_template_path().parent().map(|p| p.to_path_buf());

    tctx.set("advanced_settings_expanded", advanced_settings_expanded);
    tctx.set(
        "advanced_settings_chevron",
        if advanced_settings_expanded {
            "\u{25be}"
        } else {
            "\u{25b8}"
        },
    );

    tctx.set("enabled", cfg.enabled);
    tctx.set("battery_saver", cfg.battery_saver);
    tctx.set("launch_at_login", cfg.launch_at_login);
    tctx.set("macos", cfg!(target_os = "macos"));
    tctx.set(
        "monitor_mode_linked",
        cfg.monitor_mode == MonitorMode::Linked,
    );
    tctx.set(
        "wallpaper_layout_spanned",
        cfg.wallpaper_layout == WallpaperLayout::SpanDisplays,
    );
    tctx.set("selected_monitor_name", cfg.selected_monitor_name());

    let monitor_status = if cfg.monitor_mode == MonitorMode::Linked {
        "Shared profile across every display.".to_string()
    } else {
        format!("Selected display: {}", cfg.selected_monitor_name())
    };
    tctx.set("monitor_status", monitor_status);

    let (source_label, source_detail, source_is_image, source_is_now_playing) =
        match &settings.color_mode {
            ColorMode::Preset(preset) => (
                format!(
                    "Preset: {}",
                    match preset {
                        ColorPreset::Original => "Original",
                        ColorPreset::Plasma => "Plasma",
                        ColorPreset::Poolside => "Poolside",
                        ColorPreset::Freedom => "Freedom",
                    }
                ),
                "Built-in palette".to_string(),
                false,
                false,
            ),
            ColorMode::ImageFile(path) => (
                "Image source".to_string(),
                path.display().to_string(),
                true,
                false,
            ),
            ColorMode::NowPlaying(source) => (
                format!("Now playing: {}", source.label()),
                "Album art from the active music app".to_string(),
                false,
                true,
            ),
        };
    tctx.set("color_source_label", source_label);
    tctx.set("color_source_detail", source_detail);
    tctx.set("color_source_is_image", source_is_image);
    tctx.set("color_source_is_now_playing", source_is_now_playing);
    tctx.set(
        "now_playing_automatic_active",
        matches!(
            settings.color_mode,
            ColorMode::NowPlaying(NowPlayingSource::Automatic)
        ),
    );
    tctx.set(
        "now_playing_apple_music_active",
        matches!(
            settings.color_mode,
            ColorMode::NowPlaying(NowPlayingSource::AppleMusic)
        ),
    );
    tctx.set(
        "now_playing_spotify_active",
        matches!(
            settings.color_mode,
            ColorMode::NowPlaying(NowPlayingSource::Spotify)
        ),
    );

    let [color_a_hex, color_b_hex, color_c_hex] = palette_hexes(settings, cfg.now_playing_palette);
    tctx.set("color_a_hex", color_a_hex);
    tctx.set("color_b_hex", color_b_hex);
    tctx.set("color_c_hex", color_c_hex);
    let accent = accent_hex(
        settings,
        cfg.ui_accent_override.as_deref(),
        cfg.now_playing_accent_hex.as_deref(),
        cfg.now_playing_palette,
    );
    tctx.set("accent", accent.clone());
    tctx.set("accent_hex", accent.clone());
    tctx.set(
        "palette_swatches",
        palette_swatches(settings, &accent, cfg.now_playing_palette),
    );
    for (index, hex) in palette_wheel_hexes(settings, cfg.now_playing_palette)
        .into_iter()
        .enumerate()
    {
        tctx.set(format!("palette_swatch_{index}_value"), hex.clone());
        tctx.set(
            format!("palette_swatch_{index}_action"),
            format!("pick_palette_swatch__{index}"),
        );
        tctx.set(format!("palette_swatch_{index}_selected"), hex == accent);
    }
    tctx.set(
        "wallpaper_brightness",
        format!("{:.2}", settings.wallpaper_brightness),
    );

    tctx.set(
        "preset_original_active",
        matches!(
            settings.color_mode,
            ColorMode::Preset(ColorPreset::Original)
        ),
    );
    tctx.set(
        "preset_plasma_active",
        matches!(settings.color_mode, ColorMode::Preset(ColorPreset::Plasma)),
    );
    tctx.set(
        "preset_poolside_active",
        matches!(
            settings.color_mode,
            ColorMode::Preset(ColorPreset::Poolside)
        ),
    );
    tctx.set(
        "preset_freedom_active",
        matches!(settings.color_mode, ColorMode::Preset(ColorPreset::Freedom)),
    );

    tctx.set("mode_label", format!("{:?}", settings.mode));
    tctx.set(
        "pressure_mode_label",
        pressure_mode_label(settings.pressure_mode),
    );
    tctx.set(
        "pressure_clear_value",
        format!("{:.2}", pressure_clear_value(settings.pressure_mode)),
    );
    tctx.set("seed_label", settings.seed.as_deref().unwrap_or("Auto"));

    tctx.set("fluid_size", settings.fluid_size.to_string());
    tctx.set(
        "fluid_frame_rate",
        format!("{:.1}", settings.fluid_frame_rate),
    );
    tctx.set("fluid_timestep", format!("{:.4}", settings.fluid_timestep));
    tctx.set("viscosity", format!("{:.2}", settings.viscosity));
    tctx.set(
        "velocity_dissipation",
        format!("{:.3}", settings.velocity_dissipation),
    );
    tctx.set(
        "diffusion_iterations",
        settings.diffusion_iterations.to_string(),
    );
    tctx.set(
        "pressure_iterations",
        settings.pressure_iterations.to_string(),
    );
    tctx.set("line_length", format!("{:.0}", settings.line_length));
    tctx.set("line_width", format!("{:.1}", settings.line_width));
    tctx.set(
        "line_begin_offset",
        format!("{:.2}", settings.line_begin_offset),
    );
    tctx.set("line_variance", format!("{:.2}", settings.line_variance));
    tctx.set("grid_spacing", settings.grid_spacing.to_string());
    tctx.set("view_scale", format!("{:.2}", settings.view_scale));
    tctx.set(
        "noise_multiplier",
        format!("{:.2}", settings.noise_multiplier),
    );

    for (index, channel) in settings.noise_channels.iter().enumerate() {
        tctx.set(
            format!("noise_channel_{index}_scale"),
            format!("{:.3}", channel.scale),
        );
        tctx.set(
            format!("noise_channel_{index}_multiplier"),
            format!("{:.3}", channel.multiplier),
        );
        tctx.set(
            format!("noise_channel_{index}_offset_increment"),
            format!("{:.4}", channel.offset_increment),
        );
    }

    tctx
}

fn palette_hexes(settings: &Settings, now_playing_palette: Option<[[f32; 3]; 3]>) -> [String; 3] {
    match settings.color_mode {
        ColorMode::Preset(ColorPreset::Original) => [
            "#0a1430".to_string(),
            "#1e5db5".to_string(),
            "#d7eef9".to_string(),
        ],
        ColorMode::Preset(ColorPreset::Plasma) => palette_hexes_from_scheme(&COLOR_SCHEME_PLASMA),
        ColorMode::Preset(ColorPreset::Poolside) => {
            palette_hexes_from_scheme(&COLOR_SCHEME_POOLSIDE)
        }
        ColorMode::Preset(ColorPreset::Freedom) => [
            "#0057b7".to_string(),
            "#2b79d0".to_string(),
            "#ffd900".to_string(),
        ],
        ColorMode::ImageFile(_) => [
            "#0a1430".to_string(),
            "#1e5db5".to_string(),
            "#d7eef9".to_string(),
        ],
        ColorMode::NowPlaying(_) => {
            if let Some(stops) = now_playing_palette {
                [
                    rgb_triplet_to_hex(stops[0]),
                    rgb_triplet_to_hex(stops[1]),
                    rgb_triplet_to_hex(stops[2]),
                ]
            } else {
                [
                    "#0a1430".to_string(),
                    "#1e5db5".to_string(),
                    "#d7eef9".to_string(),
                ]
            }
        }
    }
}

fn palette_hexes_from_scheme(scheme: &[f32; 24]) -> [String; 3] {
    [
        rgb_triplet_to_hex([scheme[0], scheme[1], scheme[2]]),
        rgb_triplet_to_hex([scheme[8], scheme[9], scheme[10]]),
        rgb_triplet_to_hex([scheme[16], scheme[17], scheme[18]]),
    ]
}

fn accent_hex(
    settings: &Settings,
    user_hex: Option<&str>,
    now_playing_hex: Option<&str>,
    now_playing_palette: Option<[[f32; 3]; 3]>,
) -> String {
    user_hex
        .or(now_playing_hex)
        .map(|hex| hex.to_string())
        .unwrap_or_else(|| palette_hexes(settings, now_playing_palette)[1].clone())
}

fn palette_swatches(
    settings: &Settings,
    accent_hex: &str,
    now_playing_palette: Option<[[f32; 3]; 3]>,
) -> Vec<TemplateContext> {
    palette_wheel_hexes(settings, now_playing_palette)
        .into_iter()
        .enumerate()
        .map(|(index, hex)| {
            let mut ctx = TemplateContext::new();
            ctx.set("value", hex.clone());
            ctx.set("action", format!("pick_palette_swatch__{index}"));
            ctx.set("selected", hex == accent_hex);
            ctx
        })
        .collect()
}

fn palette_wheel_hexes(
    settings: &Settings,
    now_playing_palette: Option<[[f32; 3]; 3]>,
) -> [String; 6] {
    match &settings.color_mode {
        ColorMode::Preset(ColorPreset::Plasma) => {
            palette_wheel_hexes_from_scheme(&COLOR_SCHEME_PLASMA)
        }
        ColorMode::Preset(ColorPreset::Poolside) => {
            palette_wheel_hexes_from_scheme(&COLOR_SCHEME_POOLSIDE)
        }
        ColorMode::Preset(preset) => palette_wheel_hexes_from_stops(palette_stops(*preset)),
        ColorMode::ImageFile(_) => {
            palette_wheel_hexes_from_stops(palette_stops(ColorPreset::Original))
        }
        ColorMode::NowPlaying(_) => {
            if let Some(stops) = now_playing_palette {
                palette_wheel_hexes_from_stops(stops)
            } else {
                palette_wheel_hexes_from_stops(palette_stops(ColorPreset::Original))
            }
        }
    }
}

fn palette_wheel_hexes_from_stops(stops: [[f32; 3]; 3]) -> [String; 6] {
    std::array::from_fn(|index| {
        let t = index as f32 / 5.0;
        rgb_triplet_to_hex(sample_three_stop_palette(stops, t))
    })
}

fn palette_wheel_hexes_from_scheme(scheme: &[f32; 24]) -> [String; 6] {
    std::array::from_fn(|index| {
        let offset = index * 4;
        rgb_triplet_to_hex([scheme[offset], scheme[offset + 1], scheme[offset + 2]])
    })
}

fn sample_three_stop_palette(stops: [[f32; 3]; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    if t <= 0.5 {
        lerp3(stops[0], stops[1], t * 2.0)
    } else {
        lerp3(stops[1], stops[2], (t - 0.5) * 2.0)
    }
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn palette_stops(preset: ColorPreset) -> [[f32; 3]; 3] {
    match preset {
        ColorPreset::Original => [[0.02, 0.08, 0.19], [0.15, 0.43, 0.80], [0.84, 0.94, 0.98]],
        ColorPreset::Plasma => [[0.24, 0.15, 0.26], [0.67, 0.21, 0.20], [0.53, 0.60, 0.72]],
        ColorPreset::Poolside => [[0.30, 0.61, 0.89], [0.55, 0.80, 0.96], [0.61, 0.82, 0.92]],
        ColorPreset::Freedom => [[0.0, 0.34, 0.72], [0.0, 0.53, 0.91], [1.0, 0.84, 0.0]],
    }
}

fn rgb_triplet_to_hex(rgb: [f32; 3]) -> String {
    let r = (rgb[0].clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (rgb[1].clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = (rgb[2].clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn pressure_mode_label(mode: PressureMode) -> String {
    match mode {
        PressureMode::Retain => "Retain".to_string(),
        PressureMode::ClearWith(value) => format!("ClearWith({value:.2})"),
    }
}

fn pressure_clear_value(mode: PressureMode) -> f32 {
    match mode {
        PressureMode::Retain => 0.0,
        PressureMode::ClearWith(value) => value,
    }
}

fn spawn_default_wallpaper_process() -> anyhow::Result<()> {
    let exe = std::env::current_exe().context("Resolve current executable")?;
    std::process::Command::new(exe)
        .spawn()
        .context("Spawn drift-wallpaper default (live wallpaper)")?;
    Ok(())
}

fn spawn_mode(flag: &str) -> anyhow::Result<()> {
    let exe = std::env::current_exe().context("Resolve current executable")?;
    std::process::Command::new(exe)
        .arg(flag)
        .spawn()
        .with_context(|| format!("Spawn renderer with {flag}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_template_parses() {
        let source = std::fs::read_to_string(settings_template_path()).unwrap();
        let component_file = parse_component_file(&source).unwrap();
        assert!(component_file.components.contains_key("SettingsRoot"));
    }
}
