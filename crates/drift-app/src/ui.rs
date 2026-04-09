use std::path::PathBuf;
use std::sync::LazyLock;
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result};
use crepuscularity_runtime::{
    parse_component_file, render_nodes_interactive, ComponentFile, CrepusMouseDispatch,
    TemplateContext, TemplateValue,
};
use drift_core::color::{ColorPalette, Preset};
use drift_core::simulation::DriftParams;
use gpui::{
    actions, bounds, div, point, px, rgb, size, App as GpuiApp, AppContext, Application, Context,
    IntoElement, KeyBinding, MouseUpEvent, ParentElement, PathPromptOptions, Render, Styled,
    Window, WindowBounds, WindowOptions,
};

use crate::{
    cli,
    config::{AppConfig, SUPPRESS_MENU_BAR_TRAY_ENV},
};

#[cfg(target_os = "macos")]
use crate::menubar;

actions!(drift_app_actions, [Quit]);

static SETTINGS_UI: LazyLock<Result<ComponentFile, String>> =
    LazyLock::new(|| parse_component_file(include_str!("../views/settings_ui.crepus")));

/// When set (e.g. by the tray "Open Settings" action), the settings window opens without
/// adding another menu bar icon—the live wallpaper process already owns the tray.
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

        let bounds = bounds(point(px(88.), px(72.)), size(px(420.), px(560.)));
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
            window_min_size: Some(size(px(380.), px(480.))),
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
        let mut g = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config mutex poisoned"))?;
        *g = cfg.clone();
        g.save()
    }

    fn apply_preset_enum(&mut self, preset: Preset, cx: &mut Context<Self>) {
        let mut cfg = self.read_config();
        let speed = cfg.params.speed;
        let scale = cfg.params.scale;
        let target_fps = cfg.params.target_fps;
        let palette = ColorPalette::preset(preset);
        cfg.params = DriftParams::from_palette(&palette, speed);
        cfg.params.scale = scale;
        cfg.params.target_fps = target_fps;
        match self.save_config(&cfg) {
            Ok(()) => cx.notify(),
            Err(e) => log::warn!("save config: {e}"),
        }
    }

    fn open_color_picker(&mut self, index: usize, _: &mut Window, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        {
            let mut cfg = self.read_config();
            let initial = match index {
                0 => cfg.params.color_a,
                1 => cfg.params.color_b,
                _ => cfg.params.color_c,
            };
            let Some(color) = macos_choose_color(initial) else {
                return;
            };
            match index {
                0 => cfg.params.color_a = color,
                1 => cfg.params.color_b = color,
                _ => cfg.params.color_c = color,
            }
            if let Err(e) = self.save_config(&cfg) {
                log::warn!("save config: {e}");
            }
            cx.notify();
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (index, cx);
            log::info!("Color picker is only available on macOS.");
        }
    }

    fn pick_image_file(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose an image".into()),
        });
        match pollster::block_on(rx) {
            Ok(Ok(Some(paths))) => {
                let Some(path) = paths.first() else {
                    return;
                };
                let mut cfg = self.read_config();
                match cli::apply_image_palette(&mut cfg, path).and_then(|_| self.save_config(&cfg))
                {
                    Ok(()) => cx.notify(),
                    Err(e) => log::warn!("image palette: {e}"),
                }
            }
            Ok(Ok(None)) | Ok(Err(_)) | Err(_) => {}
        }
    }

    fn apply_current_wallpaper(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(target_os = "macos")]
        {
            let mut cfg = self.read_config();
            match cli::apply_current_wallpaper_palette(&mut cfg)
                .and_then(|_| self.save_config(&cfg))
            {
                Ok(()) => cx.notify(),
                Err(e) => log::warn!("wallpaper image: {e}"),
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = cx;
            log::info!("System wallpaper sampling is only implemented on macOS.");
        }
    }

    fn apply_wallpaper_screenshot(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(target_os = "macos")]
        {
            let mut cfg = self.read_config();
            match cli::apply_wallpaper_screenshot_palette(&mut cfg)
                .and_then(|_| self.save_config(&cfg))
            {
                Ok(()) => cx.notify(),
                Err(e) => log::warn!("screenshot palette: {e}"),
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = cx;
        }
    }

    fn open_background(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Err(e) = spawn_default_wallpaper_process() {
            log::warn!("launch wallpaper: {e}");
        }
        cx.notify();
    }

    fn open_preview(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Err(e) = spawn_mode("--preview") {
            log::warn!("launch preview: {e}");
        }
        cx.notify();
    }

    fn toggle_wallpaper_enabled(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut cfg = self.read_config();
        cfg.enabled = !cfg.enabled;
        if let Err(e) = self.save_config(&cfg) {
            log::warn!("save config: {e}");
        }
        cx.notify();
    }

    fn toggle_launch_at_login(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        {
            use crate::launch_agent;

            let mut cfg = self.read_config();
            let next = !cfg.launch_at_login;
            cfg.launch_at_login = next;
            let agent_res = if next {
                launch_agent::install()
            } else {
                launch_agent::uninstall()
            };
            if let Err(err) = agent_res.and_then(|_| self.save_config(&cfg)) {
                log::warn!("open at login: {err}");
                cfg.launch_at_login = !next;
                let _ = self.save_config(&cfg);
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = cx;
            log::info!("Launch at login is only available on macOS.");
            return;
        }
        cx.notify();
    }

    fn close_window(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.quit();
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
        match action {
            "close_window" => self.close_window(event, window, cx),
            "toggle_wallpaper_enabled" => self.toggle_wallpaper_enabled(event, window, cx),
            "toggle_launch_at_login" => self.toggle_launch_at_login(event, window, cx),
            "pick_image_file" => self.pick_image_file(event, window, cx),
            "apply_wallpaper_image" => self.apply_current_wallpaper(event, window, cx),
            "apply_wallpaper_screenshot" => self.apply_wallpaper_screenshot(event, window, cx),
            "open_background" => self.open_background(event, window, cx),
            "open_preview" => self.open_preview(event, window, cx),
            "swatch_0" => self.open_color_picker(0, window, cx),
            "swatch_1" => self.open_color_picker(1, window, cx),
            "swatch_2" => self.open_color_picker(2, window, cx),
            "preset_flux_original" => self.apply_preset_enum(Preset::FluxOriginal, cx),
            "preset_flux_plasma" => self.apply_preset_enum(Preset::FluxPlasma, cx),
            "preset_flux_poolside" => self.apply_preset_enum(Preset::FluxPoolside, cx),
            "preset_flux_freedom" => self.apply_preset_enum(Preset::FluxFreedom, cx),
            "preset_ocean" => self.apply_preset_enum(Preset::Ocean, cx),
            "preset_sunset" => self.apply_preset_enum(Preset::Sunset, cx),
            "preset_forest" => self.apply_preset_enum(Preset::Forest, cx),
            "preset_lava" => self.apply_preset_enum(Preset::Lava, cx),
            "preset_midnight" => self.apply_preset_enum(Preset::Midnight, cx),
            "preset_monochrome" => self.apply_preset_enum(Preset::Monochrome, cx),
            other => log::warn!("unknown crepus action: {other}"),
        }
    }
}

impl Render for DriftUi {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let cfg = self.read_config();

        let comp_file = match SETTINGS_UI.as_ref() {
            Ok(c) => c,
            Err(e) => {
                return div()
                    .w_full()
                    .h_full()
                    .p(px(16.))
                    .text_color(rgb(0xf87171))
                    .child(format!("Settings template error:\n{e}"))
                    .into_any_element();
            }
        };

        let Some(root) = comp_file.components.get("SettingsRoot") else {
            return div()
                .w_full()
                .h_full()
                .p(px(16.))
                .text_color(rgb(0xf87171))
                .child("SettingsRoot component missing from settings_ui.crepus")
                .into_any_element();
        };

        let mut tctx = TemplateContext::new();
        tctx.base_dir = Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("views"));
        tctx.virtual_files.insert(
            "settings_ui.crepus".into(),
            include_str!("../views/settings_ui.crepus").to_string(),
        );

        tctx.set("enabled", TemplateValue::Bool(cfg.enabled));
        #[cfg(target_os = "macos")]
        tctx.set("macos", TemplateValue::Bool(true));
        #[cfg(not(target_os = "macos"))]
        tctx.set("macos", TemplateValue::Bool(false));
        tctx.set("launch_at_login", TemplateValue::Bool(cfg.launch_at_login));
        tctx.set("color_a_hex", rgb_hex(cfg.params.color_a));
        tctx.set("color_b_hex", rgb_hex(cfg.params.color_b));
        tctx.set("color_c_hex", rgb_hex(cfg.params.color_c));

        render_nodes_interactive(&root.nodes, &tctx, cx)
    }
}

fn rgb_hex(c: [f32; 3]) -> TemplateValue {
    let r = (c[0].clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (c[1].clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = (c[2].clamp(0.0, 1.0) * 255.0).round() as u8;
    TemplateValue::Str(format!("#{r:02x}{g:02x}{b:02x}"))
}

/// Spawn the default process (desktop wallpaper engine — same as double-clicking the app).
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

#[cfg(target_os = "macos")]
fn macos_choose_color(initial: [f32; 3]) -> Option<[f32; 3]> {
    fn to_apple(c: f32) -> i64 {
        ((c.clamp(0.0, 1.0) * 65535.0).round() as i64).clamp(0, 65535)
    }
    let r = to_apple(initial[0]);
    let g = to_apple(initial[1]);
    let b = to_apple(initial[2]);
    let script = format!(
        "set c to choose color default color {{{r}, {g}, {b}}}\n\
         return (item 1 of c as text) & \",\" & (item 2 of c as text) & \",\" & (item 3 of c as text)"
    );
    let output = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = s.trim().split(',').map(|p| p.trim()).collect();
    if parts.len() != 3 {
        return None;
    }
    let parse = |p: &str| -> Option<f32> {
        let n: i64 = p.parse().ok()?;
        Some((n as f32 / 65535.0).clamp(0.0, 1.0))
    };
    Some([parse(parts[0])?, parse(parts[1])?, parse(parts[2])?])
}
