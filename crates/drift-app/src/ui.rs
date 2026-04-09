use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result};
use crepuscularity_runtime::{
    parse_component_file, render_nodes_interactive, ComponentFile, CrepusMouseDispatch,
    TemplateContext,
};
use drift_core::{ColorMode, ColorPreset, Mode, PressureMode, Settings};
use gpui::{
    actions, bounds, div, point, px, rgb, size, App as GpuiApp, AppContext, Application, Context,
    IntoElement, KeyBinding, MouseUpEvent, ParentElement, PathPromptOptions, Render, Styled,
    Window, WindowBounds, WindowOptions,
};

use crate::{
    cli,
    config::{AppConfig, MonitorMode, SUPPRESS_MENU_BAR_TRAY_ENV},
};

#[cfg(target_os = "macos")]
use crate::menubar;

actions!(drift_app_actions, [Quit]);

pub fn run_ui(initial_config: AppConfig) -> Result<()> {
    let shared = Arc::new(Mutex::new(initial_config));
    let shared_window = Arc::clone(&shared);
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

        let bounds = bounds(point(px(88.), px(72.)), size(px(820.), px(860.)));
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
            window_min_size: Some(size(px(640.), px(700.))),
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
    config: Arc<Mutex<AppConfig>>,
}

impl DriftUi {
    fn new(config: Arc<Mutex<AppConfig>>, _cx: &mut Context<Self>) -> Self {
        Self { config }
    }

    fn read_config(&self) -> AppConfig {
        self.config.lock().map(|g| g.clone()).unwrap_or_default()
    }

    fn save_config(&self, cfg: &AppConfig) -> Result<()> {
        let mut normalized = cfg.clone();
        normalized.sync_linked_monitors();
        let mut guard = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config mutex poisoned"))?;
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
            "set_preset_original" => self.update_settings(cx, |settings| {
                settings.color_mode = ColorMode::Preset(ColorPreset::Original)
            }),
            "set_preset_plasma" => self.update_settings(cx, |settings| {
                settings.color_mode = ColorMode::Preset(ColorPreset::Plasma)
            }),
            "set_preset_poolside" => self.update_settings(cx, |settings| {
                settings.color_mode = ColorMode::Preset(ColorPreset::Poolside)
            }),
            "set_preset_freedom" => self.update_settings(cx, |settings| {
                settings.color_mode = ColorMode::Preset(ColorPreset::Freedom)
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
            self.modify_config(cx, |cfg| cli::apply_image_color_mode(cfg, path));
        }
    }

    fn apply_current_wallpaper(&mut self, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        {
            self.modify_config(cx, cli::apply_current_wallpaper_color_mode);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = cx;
    }

    fn apply_wallpaper_screenshot(&mut self, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        {
            self.modify_config(cx, cli::apply_wallpaper_screenshot_color_mode);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = cx;
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let cfg = self.read_config();
        let source = build_settings_template(&cfg);
        let component_file: ComponentFile = match parse_component_file(&source) {
            Ok(file) => file,
            Err(error) => {
                return div()
                    .w_full()
                    .h_full()
                    .p(px(16.))
                    .text_color(rgb(0xf87171))
                    .child(format!("Settings template error:\n{error}"))
                    .into_any_element();
            }
        };

        let Some(root) = component_file.components.get("SettingsRoot") else {
            return div()
                .w_full()
                .h_full()
                .p(px(16.))
                .text_color(rgb(0xf87171))
                .child("SettingsRoot component missing from generated settings")
                .into_any_element();
        };

        let tctx = TemplateContext::new();
        render_nodes_interactive(&root.nodes, &tctx, cx)
    }
}

fn build_settings_template(cfg: &AppConfig) -> String {
    let settings = cfg.active_profile();
    let monitor_ids: Vec<_> = cfg.monitors.keys().cloned().collect();
    let selected_monitor = cfg.selected_monitor_id();

    let mut out = String::new();
    out.push_str(
        "+++\n+++\n\n--- SettingsRoot\ndiv w-full h-full flex flex-col min-h-0 bg-zinc-950 text-zinc-100 text-sm\n  div shrink-0 flex items-center justify-between px-4 py-3 border-b border-zinc-800\n    div text-lg font-semibold tracking-tight\n      \"Flux Wallpaper\"\n    div px-2 py-1 rounded-md text-zinc-400 cursor-pointer @mouseup=close_window\n      \"Close\"\n\n  div flex-1 min-h-0 overflow-y-scroll px-4 py-4 flex flex-col gap-4\n",
    );

    out.push_str(&card(
        "Session",
        &[
            action_row(
                "Wallpaper",
                if cfg.enabled { "Live" } else { "Paused" },
                "toggle_wallpaper_enabled",
            ),
            action_row(
                "Monitor mode",
                if cfg.monitor_mode == MonitorMode::Linked {
                    "Linked"
                } else {
                    "Independent"
                },
                "toggle_link_mode",
            ),
            action_row("Start wallpaper", "Desktop windows", "open_background"),
            action_row("Open preview", "Preview window", "open_preview"),
            action_row(
                "Open at login",
                if cfg.launch_at_login { "On" } else { "Off" },
                "toggle_launch_at_login",
            ),
        ]
        .join(""),
    ));

    let mut monitor_markup = String::from("div flex flex-wrap gap-2\n");
    if monitor_ids.is_empty() {
        monitor_markup.push_str(&button("No monitors discovered yet", None, false));
    } else {
        for (index, monitor_id) in monitor_ids.iter().enumerate() {
            let monitor = &cfg.monitors[monitor_id];
            let active = selected_monitor == Some(monitor_id.as_str());
            monitor_markup.push_str(&button(
                &monitor.name_hint,
                Some(&format!("select_monitor__{index}")),
                active,
            ));
        }
    }
    out.push_str(&card("Monitors", &monitor_markup));

    let color_source_label = match &settings.color_mode {
        ColorMode::Preset(ColorPreset::Original) => "Original preset".to_string(),
        ColorMode::Preset(ColorPreset::Plasma) => "Plasma preset".to_string(),
        ColorMode::Preset(ColorPreset::Poolside) => "Poolside preset".to_string(),
        ColorMode::Preset(ColorPreset::Freedom) => "Freedom preset".to_string(),
        ColorMode::ImageFile(path) => format!("Image: {}", path.display()),
    };
    let mut color_markup = String::new();
    color_markup.push_str("div flex flex-col gap-3\n");
    color_markup.push_str("  div text-xs text-zinc-400\n");
    color_markup.push_str(&format!("    {}\n", quoted(&color_source_label)));
    color_markup.push_str("  div flex flex-wrap gap-2\n");
    color_markup.push_str(&button(
        "Original",
        Some("set_preset_original"),
        matches!(
            settings.color_mode,
            ColorMode::Preset(ColorPreset::Original)
        ),
    ));
    color_markup.push_str(&button(
        "Plasma",
        Some("set_preset_plasma"),
        matches!(settings.color_mode, ColorMode::Preset(ColorPreset::Plasma)),
    ));
    color_markup.push_str(&button(
        "Poolside",
        Some("set_preset_poolside"),
        matches!(
            settings.color_mode,
            ColorMode::Preset(ColorPreset::Poolside)
        ),
    ));
    color_markup.push_str(&button(
        "Freedom",
        Some("set_preset_freedom"),
        matches!(settings.color_mode, ColorMode::Preset(ColorPreset::Freedom)),
    ));
    color_markup.push_str(&button("Choose image…", Some("pick_image_file"), false));
    color_markup.push_str(&button(
        "Use wallpaper",
        Some("apply_wallpaper_image"),
        false,
    ));
    color_markup.push_str(&button(
        "Use screenshot",
        Some("apply_wallpaper_screenshot"),
        false,
    ));
    out.push_str(&card("Color Source", &color_markup));

    out.push_str(&card(
        "Debug & Seed",
        &[
            action_row("Render mode", &format!("{:?}", settings.mode), "cycle_mode"),
            action_row(
                "Pressure mode",
                &pressure_mode_label(settings.pressure_mode),
                "toggle_pressure_mode",
            ),
            adjust_row(
                "Clear pressure",
                &format!("{:.2}", pressure_clear_value(settings.pressure_mode)),
                "pressure_clear_down",
                "pressure_clear_up",
            ),
            action_row(
                "Seed",
                settings.seed.as_deref().unwrap_or("Auto"),
                "seed_randomize",
            ),
            action_row("Clear seed", "Use runtime entropy", "seed_clear"),
        ]
        .join(""),
    ));

    out.push_str(&card(
        "Fluid",
        &[
            adjust_field("Fluid size", settings.fluid_size, "fluid_size"),
            adjust_field("Fluid FPS", settings.fluid_frame_rate, "fluid_frame_rate"),
            adjust_field("Fluid timestep", settings.fluid_timestep, "fluid_timestep"),
            adjust_field("Viscosity", settings.viscosity, "viscosity"),
            adjust_field(
                "Velocity dissipation",
                settings.velocity_dissipation,
                "velocity_dissipation",
            ),
            adjust_field(
                "Diffusion iterations",
                settings.diffusion_iterations,
                "diffusion_iterations",
            ),
            adjust_field(
                "Pressure iterations",
                settings.pressure_iterations,
                "pressure_iterations",
            ),
        ]
        .join(""),
    ));

    out.push_str(&card(
        "Lines",
        &[
            adjust_field("Line length", settings.line_length, "line_length"),
            adjust_field("Line width", settings.line_width, "line_width"),
            adjust_field(
                "Line begin offset",
                settings.line_begin_offset,
                "line_begin_offset",
            ),
            adjust_field("Line variance", settings.line_variance, "line_variance"),
            adjust_field("Grid spacing", settings.grid_spacing, "grid_spacing"),
            adjust_field("View scale", settings.view_scale, "view_scale"),
        ]
        .join(""),
    ));

    out.push_str(&card(
        "Noise",
        &[
            adjust_field(
                "Noise multiplier",
                settings.noise_multiplier,
                "noise_multiplier",
            ),
            noise_channel_row(settings, 0),
            noise_channel_row(settings, 1),
            noise_channel_row(settings, 2),
        ]
        .join(""),
    ));

    out
}

fn card(title: &str, inner: &str) -> String {
    format!(
        "    div rounded-lg border border-zinc-800 bg-zinc-900 p-4 flex flex-col gap-3\n      div text-xs font-semibold uppercase tracking-wider text-zinc-500\n        {}\n{}",
        quoted(title),
        indent(inner, 3)
    )
}

fn action_row(label: &str, value: &str, action: &str) -> String {
    format!(
        "div flex items-center justify-between gap-4\n  div flex flex-col min-w-0 flex-1\n    div text-sm font-medium\n      {}\n    div text-xs text-zinc-500\n      {}\n  div px-3 py-2 rounded-md bg-zinc-800 border border-zinc-700 text-xs font-medium text-zinc-200 cursor-pointer @mouseup={}\n    \"Apply\"\n",
        quoted(label),
        quoted(value),
        action
    )
}

fn adjust_row(label: &str, value: &str, down_action: &str, up_action: &str) -> String {
    format!(
        "div flex items-center justify-between gap-4\n  div flex flex-col min-w-0 flex-1\n    div text-sm font-medium\n      {}\n    div text-xs text-zinc-500\n      {}\n  div flex items-center gap-2 shrink-0\n    div w-8 h-8 rounded-md bg-zinc-800 border border-zinc-700 text-zinc-200 cursor-pointer flex items-center justify-center @mouseup={}\n      \"-\"\n    div min-w-20 text-right text-xs text-zinc-300\n      {}\n    div w-8 h-8 rounded-md bg-zinc-800 border border-zinc-700 text-zinc-200 cursor-pointer flex items-center justify-center @mouseup={}\n      \"+\"\n",
        quoted(label),
        quoted(value),
        down_action,
        quoted(value),
        up_action
    )
}

fn adjust_field<T: std::fmt::Display>(label: &str, value: T, field: &str) -> String {
    adjust_row(
        label,
        &value.to_string(),
        &format!("adjust__{field}__down"),
        &format!("adjust__{field}__up"),
    )
}

fn noise_channel_row(settings: &Settings, index: usize) -> String {
    let channel = settings.noise_channels.get(index);
    let scale = channel.map(|c| c.scale).unwrap_or_default();
    let multiplier = channel.map(|c| c.multiplier).unwrap_or_default();
    let offset = channel.map(|c| c.offset_increment).unwrap_or_default();
    let mut markup = String::new();
    markup.push_str("div rounded-md border border-zinc-800 bg-zinc-950 p-3 flex flex-col gap-3\n");
    markup.push_str("  div text-xs font-semibold uppercase tracking-wide text-zinc-500\n");
    markup.push_str(&format!(
        "    {}\n",
        quoted(&format!("Channel {}", index + 1))
    ));
    markup.push_str(&indent(
        &adjust_row(
            "Scale",
            &format!("{scale:.3}"),
            &format!("noise__{index}__scale__down"),
            &format!("noise__{index}__scale__up"),
        ),
        1,
    ));
    markup.push_str(&indent(
        &adjust_row(
            "Multiplier",
            &format!("{multiplier:.3}"),
            &format!("noise__{index}__multiplier__down"),
            &format!("noise__{index}__multiplier__up"),
        ),
        1,
    ));
    markup.push_str(&indent(
        &adjust_row(
            "Offset increment",
            &format!("{offset:.4}"),
            &format!("noise__{index}__offset_increment__down"),
            &format!("noise__{index}__offset_increment__up"),
        ),
        1,
    ));
    markup
}

fn button(label: &str, action: Option<&str>, active: bool) -> String {
    let style = if active {
        "px-3 py-2 rounded-md bg-blue-600 text-xs font-semibold text-white cursor-pointer"
    } else {
        "px-3 py-2 rounded-md bg-zinc-800 border border-zinc-700 text-xs font-medium text-zinc-200 cursor-pointer"
    };
    match action {
        Some(action) => format!(
            "    div {style} @mouseup={action}\n      {}\n",
            quoted(label)
        ),
        None => format!("    div {style}\n      {}\n", quoted(label)),
    }
}

fn indent(input: &str, level: usize) -> String {
    let prefix = "  ".repeat(level);
    input
        .lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn quoted(input: &str) -> String {
    format!("\"{}\"", input.replace('\\', "\\\\").replace('"', "\\\""))
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
