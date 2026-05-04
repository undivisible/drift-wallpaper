mod cli;
mod color_picker;
mod config;
mod crepus_interactive;
mod crepus_settings_render;
mod launch_agent;
mod media_art;
mod now_playing;
mod ui;

#[cfg(target_os = "macos")]
mod menubar;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use drift_core::{FluxRenderer, Settings};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    monitor::MonitorHandle,
    window::{Window, WindowAttributes, WindowId},
};

use crate::config::{AppConfig, DiscoveredMonitor, MonitorMode, WallpaperLayout};
use drift_core::ColorMode;

fn init_logging() {
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    builder.filter_module("wgpu_core::device::resource", log::LevelFilter::Warn);
    builder.filter_module("wgpu_hal", log::LevelFilter::Warn);
    builder.init();
}

#[cfg(not(target_os = "macos"))]
fn main() -> Result<()> {
    init_logging();
    let mut config = AppConfig::load();
    let action = cli::apply_cli_args(&mut config)?;
    match action {
        cli::StartupAction::Exit => Ok(()),
        cli::StartupAction::Run { mode } => match mode {
            cli::RunMode::Ui => ui::run_ui(config),
            cli::RunMode::Background => run_app(Arc::new(Mutex::new(config)), true),
            cli::RunMode::Preview => run_app(Arc::new(Mutex::new(config)), false),
        },
    }
}

#[cfg(target_os = "macos")]
fn main() -> Result<()> {
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    use objc2_foundation::MainThreadMarker;

    init_logging();
    let mut config = AppConfig::load();
    let action = cli::apply_cli_args(&mut config)?;
    match action {
        cli::StartupAction::Exit => Ok(()),
        cli::StartupAction::Run { mode } => match mode {
            cli::RunMode::Ui => ui::run_ui(config),
            cli::RunMode::Background => {
                let mtm = unsafe { MainThreadMarker::new_unchecked() };
                let app = NSApplication::sharedApplication(mtm);
                app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
                let shared = Arc::new(Mutex::new(config));
                let _status_item = menubar::create_status_item(mtm, Arc::clone(&shared));
                run_app(shared, true)
            }
            cli::RunMode::Preview => run_app(Arc::new(Mutex::new(config)), false),
        },
    }
}

struct DisplayWindow {
    id: WindowId,
    window: Arc<Window>,
    monitor_id: String,
    renderer: FluxRenderer,
    applied_settings: Settings,
    /// When artwork is driven by now playing, re-apply renderer settings even if `Settings`
    /// compares equal (e.g. cache path hash collision or overwritten file at same path).
    applied_now_playing_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WindowSignature {
    Preview,
    Wallpaper {
        layout: WallpaperLayout,
        monitor_ids: Vec<String>,
    },
}

fn run_app(config: Arc<Mutex<AppConfig>>, wallpaper_mode: bool) -> Result<()> {
    let event_loop = EventLoop::new()?;
    // Start in Wait mode; about_to_wait switches to WaitUntil once windows exist.
    // This prevents the busy-poll that was consuming a full CPU core even at idle.
    event_loop.set_control_flow(ControlFlow::Wait);

    struct App {
        config: Arc<Mutex<AppConfig>>,
        wallpaper_mode: bool,
        windows: Vec<DisplayWindow>,
        window_signature: Option<WindowSignature>,
        last_config_refresh: Instant,
        last_config_modified: Option<SystemTime>,
        /// Last merged now-playing poll source — clears artwork when any display's mode changes it.
        wallpaper_profile_np_source_seen: Option<drift_core::NowPlayingSource>,
        now_playing_key: Option<String>,
        now_playing_snapshot: Option<media_art::NowPlayingSnapshot>,
        now_playing_controller: Option<now_playing::NowPlayingController>,
        now_playing_updates_rx: std::sync::mpsc::Receiver<now_playing::NowPlayingUpdate>,
    }

    fn sync_windows(app: &mut App, event_loop: &ActiveEventLoop) {
        let (desired_signature, window_specs, monitor_handles) =
            build_window_specs(app, event_loop);

        if app.window_signature.as_ref() != Some(&desired_signature) {
            app.windows.clear();
            app.window_signature = Some(desired_signature);
        }

        if app.windows.is_empty() {
            create_windows(app, event_loop, window_specs, monitor_handles);
        }
    }

    fn build_window_specs(
        app: &App,
        event_loop: &ActiveEventLoop,
    ) -> (
        WindowSignature,
        Vec<(DiscoveredMonitor, WindowAttributes)>,
        Vec<MonitorHandle>,
    ) {
        if app.wallpaper_mode {
            let mut monitors: Vec<_> = event_loop.available_monitors().collect();
            if monitors.is_empty() {
                if let Some(primary) = event_loop.primary_monitor() {
                    monitors.push(primary);
                }
            }
            monitors.sort_by_key(|m| (m.position().x, m.position().y));
            let monitor_handles: Vec<MonitorHandle> = monitors.clone();

            log::debug!(
                "Wallpaper: {} display(s) from winit (sorted left-to-right, top-to-bottom by position).",
                monitor_handles.len()
            );

            let discovered: Vec<_> = monitors.iter().map(discover_monitor).collect();
            if let Ok(mut cfg) = app.config.lock() {
                if cfg.ensure_monitors(&discovered) {
                    let _ = cfg.save();
                }
            }

            let wallpaper_layout = app
                .config
                .lock()
                .map(|cfg| cfg.wallpaper_layout)
                .unwrap_or(WallpaperLayout::PerMonitor);

            if wallpaper_layout == WallpaperLayout::SpanDisplays && !monitors.is_empty() {
                let (position, size) = combined_monitor_bounds(&monitors)
                    .unwrap_or_else(|| (monitors[0].position(), monitors[0].size()));
                (
                    WindowSignature::Wallpaper {
                        layout: wallpaper_layout,
                        monitor_ids: vec!["span-displays".to_string()],
                    },
                    vec![(
                        DiscoveredMonitor {
                            id: "span-displays".to_string(),
                            name_hint: "All displays".to_string(),
                        },
                        WindowAttributes::default()
                            .with_title("drift-wallpaper")
                            .with_decorations(false)
                            .with_transparent(false)
                            .with_resizable(false)
                            .with_position(position)
                            .with_inner_size(size),
                    )],
                    monitor_handles,
                )
            } else {
                let window_specs = monitors
                    .into_iter()
                    .map(|monitor| {
                        let size = monitor.size();
                        let position = monitor.position();
                        (
                            discover_monitor(&monitor),
                            WindowAttributes::default()
                                .with_title("drift-wallpaper")
                                .with_decorations(false)
                                .with_transparent(false)
                                .with_resizable(false)
                                .with_position(position)
                                .with_inner_size(size),
                        )
                    })
                    .collect::<Vec<_>>();
                let monitor_ids = window_specs
                    .iter()
                    .map(|(monitor, _)| monitor.id.clone())
                    .collect();
                (
                    WindowSignature::Wallpaper {
                        layout: wallpaper_layout,
                        monitor_ids,
                    },
                    window_specs,
                    monitor_handles,
                )
            }
        } else {
            (
                WindowSignature::Preview,
                vec![(
                    DiscoveredMonitor {
                        id: "preview".to_string(),
                        name_hint: "Preview".to_string(),
                    },
                    WindowAttributes::default()
                        .with_title("Drift Wallpaper Preview")
                        .with_inner_size(LogicalSize::new(1280.0, 720.0)),
                )],
                Vec::new(),
            )
        }
    }

    fn create_windows(
        app: &mut App,
        event_loop: &ActiveEventLoop,
        window_specs: Vec<(DiscoveredMonitor, WindowAttributes)>,
        monitor_handles: Vec<MonitorHandle>,
    ) {
        let wallpaper_layout = app
            .config
            .lock()
            .map(|cfg| cfg.wallpaper_layout)
            .unwrap_or(WallpaperLayout::PerMonitor);

        for (i, (monitor, attrs)) in window_specs.into_iter().enumerate() {
            let window = match event_loop.create_window(attrs) {
                Ok(window) => Arc::new(window),
                Err(error) => {
                    log::error!("Failed to create window: {error}");
                    continue;
                }
            };

            #[cfg(target_os = "macos")]
            if app.wallpaper_mode {
                set_desktop_window_level(window.as_ref());
                match wallpaper_layout {
                    WallpaperLayout::PerMonitor => {
                        if let Some(h) = monitor_handles.get(i) {
                            macos_snap_wallpaper_window_to_monitor(window.as_ref(), h);
                        } else {
                            log::warn!(
                                "macOS: missing MonitorHandle for wallpaper window index {i} ({})",
                                monitor.name_hint
                            );
                        }
                    }
                    WallpaperLayout::SpanDisplays => {
                        if i == 0 {
                            macos_snap_wallpaper_window_to_union_of_screens(window.as_ref());
                        }
                    }
                }
            }

            let (settings, battery_saver) = match app.config.lock() {
                Ok(cfg) => {
                    let settings = if app.wallpaper_mode
                        && cfg.wallpaper_layout == WallpaperLayout::SpanDisplays
                        && !cfg.monitors.is_empty()
                    {
                        cfg.wallpaper_profile().clone()
                    } else {
                        cfg.settings_for_monitor(&monitor.id)
                    };
                    (settings, cfg.battery_saver)
                }
                Err(_) => (Settings::default(), false),
            };

            let settings = materialize_runtime_settings(
                settings,
                app.now_playing_snapshot.as_ref(),
                battery_saver,
            );

            match create_renderer(Arc::clone(&window), settings.clone()) {
                Ok(mut renderer) => {
                    #[cfg(target_os = "macos")]
                    if app.wallpaper_mode
                        && wallpaper_layout == WallpaperLayout::SpanDisplays
                        && i == 0
                    {
                        sync_flux_renderer_to_wallpaper_window(window.as_ref(), &mut renderer);
                    }
                    app.windows.push(DisplayWindow {
                        id: window.id(),
                        window,
                        monitor_id: monitor.id,
                        renderer,
                        applied_settings: settings,
                        applied_now_playing_key: None,
                    });
                }
                Err(error) => log::error!("Failed to create renderer: {error}"),
            }
        }
    }

    impl ApplicationHandler for App {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            sync_windows(self, event_loop);
        }

        fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
            match event {
                WindowEvent::Resized(size) => {
                    if let Some(display) = self.windows.iter_mut().find(|display| display.id == id)
                    {
                        let logical = size.to_logical::<u32>(display.window.scale_factor());
                        display.renderer.resize(
                            logical.width,
                            logical.height,
                            size.width,
                            size.height,
                        );
                    }
                }
                WindowEvent::CloseRequested => {
                    self.windows.retain(|display| display.id != id);
                    if self.windows.is_empty() {
                        event_loop.exit();
                    }
                }
                WindowEvent::RedrawRequested => {
                    let enabled = self.config.lock().map(|cfg| cfg.enabled).unwrap_or(true);
                    if !enabled {
                        return;
                    }

                    if let Some(display) = self.windows.iter_mut().find(|display| display.id == id)
                    {
                        if !display.renderer.render() {
                            let size = display.window.inner_size();
                            let logical = size.to_logical::<u32>(display.window.scale_factor());
                            display.renderer.resize(
                                logical.width,
                                logical.height,
                                size.width,
                                size.height,
                            );
                        }
                    }
                }
                _ => {}
            }
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            // Refresh config from disk at most every 500 ms.
            if self.last_config_refresh.elapsed() >= Duration::from_millis(500) {
                let modified = std::fs::metadata(AppConfig::config_path())
                    .and_then(|meta| meta.modified())
                    .ok();
                if modified != self.last_config_modified {
                    if let Ok(latest) = AppConfig::try_load() {
                        if let Ok(mut current) = self.config.lock() {
                            *current = latest;
                        }
                    }
                    self.last_config_modified = modified;
                }
                self.last_config_refresh = Instant::now();
            }

            // Single config snapshot for the entire frame — avoids repeated mutex churn.
            let cfg = match self.config.lock() {
                Ok(g) => g.clone(),
                Err(_) => return,
            };

            let current_np_source = cfg.now_playing_poll_source();

            while let Ok(update) = self.now_playing_updates_rx.try_recv() {
                match update.snapshot {
                    Some(snapshot) => {
                        self.now_playing_key = Some(snapshot.key.clone());
                        self.now_playing_snapshot = Some(snapshot);
                    }
                    None => {
                        self.now_playing_key = None;
                        self.now_playing_snapshot = None;
                    }
                }
            }

            // Only reset in-memory artwork when the *merged* poll source actually changes after we
            // already had one (e.g. Spotify → Apple Music, or now playing → off). On a cold start
            // `wallpaper_profile_np_source_seen` is `None` while `current_np_source` is
            // `Some(...)` — clearing here ran *after* draining the worker channel and discarded
            // every freshly received snapshot, so the wallpaper never picked up new tracks.
            if self.wallpaper_profile_np_source_seen != current_np_source {
                if self.wallpaper_profile_np_source_seen.is_some() {
                    self.now_playing_key = None;
                    self.now_playing_snapshot = None;
                    if let Some(controller) = &self.now_playing_controller {
                        controller.request_refresh();
                    }
                }
                self.wallpaper_profile_np_source_seen = current_np_source;
            }

            sync_windows(self, event_loop);

            // Push updated settings to each renderer.
            for display in &mut self.windows {
                let mut settings = if self.wallpaper_mode
                    && cfg.wallpaper_layout == WallpaperLayout::SpanDisplays
                    && !cfg.monitors.is_empty()
                {
                    cfg.wallpaper_profile().clone()
                } else {
                    cfg.settings_for_monitor(&display.monitor_id)
                };

                settings = materialize_runtime_settings(
                    settings,
                    self.now_playing_snapshot.as_ref(),
                    cfg.battery_saver,
                );

                let uses_now_playing =
                    display_uses_now_playing_colors(&cfg, &display.monitor_id, self.wallpaper_mode);
                let np_key = uses_now_playing
                    .then(|| self.now_playing_key.clone())
                    .flatten();
                let np_key_changed = display.applied_now_playing_key != np_key;

                if display.applied_settings != settings || np_key_changed {
                    match display.renderer.set_settings(settings.clone()) {
                        Ok(()) => {
                            display.applied_settings = settings;
                            display.applied_now_playing_key = np_key;
                        }
                        Err(error) => log::warn!("update renderer settings: {error}"),
                    }
                }
            }

            // Schedule next frame via WaitUntil instead of busy-polling.
            // This allows the OS to sleep the process between frames, dropping
            // CPU usage from ~100% (Poll) to < 2% during normal animation.
            if cfg.enabled {
                for display in &self.windows {
                    display.window.request_redraw();
                }
                let fps = effective_wallpaper_fps(&cfg);
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_secs_f32(1.0 / fps),
                ));
            } else {
                // Wallpaper paused — sleep until re-enabled (no redraws needed).
                event_loop.set_control_flow(ControlFlow::Wait);
            }
        }
    }

    let (now_playing_controller, now_playing_updates_rx) =
        now_playing::spawn_now_playing_worker(Arc::clone(&config));
    let mut app = App {
        config,
        wallpaper_mode,
        windows: Vec::new(),
        window_signature: None,
        last_config_refresh: Instant::now(),
        last_config_modified: None,
        now_playing_key: None,
        wallpaper_profile_np_source_seen: None,
        now_playing_snapshot: None,
        now_playing_controller: Some(now_playing_controller),
        now_playing_updates_rx,
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn sync_flux_renderer_to_wallpaper_window(window: &Window, renderer: &mut FluxRenderer) {
    let physical = window.inner_size();
    let logical = physical.to_logical::<u32>(window.scale_factor());
    renderer.resize(
        logical.width,
        logical.height,
        physical.width,
        physical.height,
    );
}

fn create_renderer(window: Arc<Window>, settings: Settings) -> Result<FluxRenderer> {
    let physical = window.inner_size();
    let logical = physical.to_logical::<u32>(window.scale_factor());
    let mut instance_descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    instance_descriptor.backends = wgpu::Backends::all();
    let instance = wgpu::Instance::new(instance_descriptor);

    let surface = unsafe {
        instance.create_surface_unsafe(
            wgpu::SurfaceTargetUnsafe::from_display_and_window(window.as_ref(), window.as_ref())
                .context("create surface target from window")?,
        )
    }
    .context("create wgpu surface")?;

    FluxRenderer::new(
        instance,
        surface,
        logical.width,
        logical.height,
        physical.width,
        physical.height,
        settings,
    )
}

fn discover_monitor(monitor: &MonitorHandle) -> DiscoveredMonitor {
    let name = monitor.name().unwrap_or_else(|| "Display".to_string());
    let size = monitor.size();
    let position = monitor.position();
    let scale = monitor.scale_factor();
    let id = format!(
        "{}-{}x{}-{}x{}-{}",
        slugify(&name),
        size.width,
        size.height,
        position.x,
        position.y,
        scale
    );

    DiscoveredMonitor {
        id,
        name_hint: name,
    }
}

fn slugify(input: &str) -> String {
    let mut slug = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if ch == ' ' || ch == '-' || ch == '_' {
            slug.push('-');
        }
    }
    if slug.is_empty() {
        "display".to_string()
    } else {
        slug
    }
}

fn combined_monitor_bounds(
    monitors: &[winit::monitor::MonitorHandle],
) -> Option<(
    winit::dpi::PhysicalPosition<i32>,
    winit::dpi::PhysicalSize<u32>,
)> {
    let rects: Vec<(i32, i32, u32, u32)> = monitors
        .iter()
        .map(|m| {
            let p = m.position();
            let s = m.size();
            (p.x, p.y, s.width, s.height)
        })
        .collect();
    combined_bounds_from_rects(&rects)
}

/// Union of monitor rectangles in physical coordinates (top-left x/y, width, height).
fn combined_bounds_from_rects(
    rects: &[(i32, i32, u32, u32)],
) -> Option<(
    winit::dpi::PhysicalPosition<i32>,
    winit::dpi::PhysicalSize<u32>,
)> {
    let &(x0, y0, w0, h0) = rects.first()?;
    let mut min_x = x0;
    let mut min_y = y0;
    let mut max_x = x0 + w0 as i32;
    let mut max_y = y0 + h0 as i32;

    for &(x, y, w, h) in rects.iter().skip(1) {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x + w as i32);
        max_y = max_y.max(y + h as i32);
    }

    Some((
        winit::dpi::PhysicalPosition::new(min_x, min_y),
        winit::dpi::PhysicalSize::new((max_x - min_x) as u32, (max_y - min_y) as u32),
    ))
}

/// Mirrors which `Settings` clone is used for a wallpaper/preview window so we know whether
/// now-playing artwork drives its colors.
fn display_uses_now_playing_colors(
    cfg: &AppConfig,
    monitor_id: &str,
    wallpaper_mode: bool,
) -> bool {
    let color_mode = if wallpaper_mode
        && cfg.wallpaper_layout == WallpaperLayout::SpanDisplays
        && !cfg.monitors.is_empty()
    {
        &cfg.wallpaper_profile().color_mode
    } else {
        match cfg.monitor_mode {
            MonitorMode::Linked => &cfg.shared_profile.color_mode,
            MonitorMode::Independent => cfg
                .monitors
                .get(monitor_id)
                .map(|m| &m.drift_settings.color_mode)
                .unwrap_or(&cfg.shared_profile.color_mode),
        }
    };
    matches!(color_mode, ColorMode::NowPlaying(_))
}

fn materialize_runtime_settings(
    mut settings: Settings,
    snapshot: Option<&media_art::NowPlayingSnapshot>,
    battery_saver: bool,
) -> Settings {
    if matches!(settings.color_mode, ColorMode::NowPlaying(_)) {
        if let Some(snapshot) = snapshot {
            settings.color_mode = ColorMode::ImageFile(snapshot.image_path.clone());
        } else {
            settings.color_mode = ColorMode::Preset(drift_core::ColorPreset::Original);
        }
    }

    if battery_saver {
        settings.fluid_frame_rate = (settings.fluid_frame_rate * 0.5).max(12.0);
        settings.fluid_timestep = (settings.fluid_timestep * 2.0).clamp(1.0 / 240.0, 1.0 / 8.0);
    }

    settings
}

/// Present rate for the wallpaper loop (matches effective settings passed to the renderer).
fn effective_wallpaper_fps(cfg: &AppConfig) -> f32 {
    let base = cfg.wallpaper_profile().fluid_frame_rate.clamp(1.0, 240.0);
    if cfg.battery_saver {
        (base * 0.5).max(12.0)
    } else {
        base
    }
}

#[cfg(target_os = "macos")]
fn set_desktop_window_level(window: &winit::window::Window) {
    use objc2_app_kit::{NSView, NSWindowCollectionBehavior, NSWindowLevel};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    const CG_DESKTOP_WINDOW_LEVEL: NSWindowLevel = i32::MIN as NSWindowLevel + 5 + 20;

    if let Ok(handle) = window.window_handle() {
        if let RawWindowHandle::AppKit(h) = handle.as_raw() {
            let ns_view = h.ns_view.as_ptr() as *const NSView;
            unsafe {
                if let Some(ns_window) = (*ns_view).window() {
                    ns_window.setLevel(CG_DESKTOP_WINDOW_LEVEL);
                    ns_window.setIgnoresMouseEvents(true);
                    let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
                        | NSWindowCollectionBehavior::Stationary
                        | NSWindowCollectionBehavior::IgnoresCycle;
                    ns_window.setCollectionBehavior(behavior);
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn macos_snap_wallpaper_window_to_monitor(window: &winit::window::Window, monitor: &MonitorHandle) {
    use objc2_app_kit::{NSScreen, NSView};
    use objc2_foundation::NSRect;
    use winit::platform::macos::MonitorHandleExtMacOS;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let maybe_frame: Option<NSRect> = match monitor.ns_screen() {
        Some(ptr) if !ptr.is_null() => Some(unsafe { (*ptr.cast::<NSScreen>()).frame() }),
        _ => {
            log::warn!(
                "macOS: no NSScreen for monitor at ({}, {}); leaving winit placement",
                monitor.position().x,
                monitor.position().y,
            );
            None
        }
    };

    if let Ok(handle) = window.window_handle() {
        if let RawWindowHandle::AppKit(h) = handle.as_raw() {
            let ns_view = h.ns_view.as_ptr() as *const NSView;
            unsafe {
                if let Some(ns_window) = (*ns_view).window() {
                    if let Some(frame) = maybe_frame {
                        ns_window.setFrame_display(frame, false);
                    }
                    ns_window.orderFrontRegardless();
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn macos_snap_wallpaper_window_to_union_of_screens(window: &winit::window::Window) {
    use objc2_app_kit::{NSScreen, NSView};
    use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!("macOS: span frame snap skipped (not on main thread)");
        return;
    };

    let screens = NSScreen::screens(mtm);
    let n = screens.count();
    if n == 0 {
        return;
    }

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for i in 0..n {
        let screen = screens.objectAtIndex(i);
        let f: NSRect = screen.frame();
        let x0 = f.origin.x;
        let y0 = f.origin.y;
        let x1 = x0 + f.size.width;
        let y1 = y0 + f.size.height;
        min_x = min_x.min(x0);
        min_y = min_y.min(y0);
        max_x = max_x.max(x1);
        max_y = max_y.max(y1);
    }

    let union = NSRect::new(
        NSPoint::new(min_x, min_y),
        NSSize::new(max_x - min_x, max_y - min_y),
    );

    if let Ok(handle) = window.window_handle() {
        if let RawWindowHandle::AppKit(h) = handle.as_raw() {
            let ns_view = h.ns_view.as_ptr() as *const NSView;
            unsafe {
                if let Some(ns_window) = (*ns_view).window() {
                    ns_window.setFrame_display(union, true);
                    ns_window.orderFrontRegardless();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, MonitorConfig, MonitorMode, WallpaperLayout};
    use drift_core::{ColorMode, ColorPreset, NowPlayingSource, Settings};

    #[test]
    fn slugify_alphanumeric_and_separators() {
        assert_eq!(slugify("LG HDR 4K"), "lg-hdr-4k");
        assert_eq!(
            slugify("Built-in Retina Display"),
            "built-in-retina-display"
        );
    }

    #[test]
    fn slugify_non_word_fallback() {
        assert_eq!(slugify("!!!"), "display");
        assert_eq!(slugify(""), "display");
    }

    #[test]
    fn combined_bounds_single_rect() {
        let r = [(0, 0, 1920, 1080)];
        let (pos, size) = combined_bounds_from_rects(&r).unwrap();
        assert_eq!(pos.x, 0);
        assert_eq!(pos.y, 0);
        assert_eq!(size.width, 1920);
        assert_eq!(size.height, 1080);
    }

    #[test]
    fn combined_bounds_two_side_by_side() {
        let r = [(0, 0, 1920, 1080), (1920, 0, 1920, 1080)];
        let (pos, size) = combined_bounds_from_rects(&r).unwrap();
        assert_eq!(pos.x, 0);
        assert_eq!(size.width, 3840);
        assert_eq!(size.height, 1080);
    }

    #[test]
    fn combined_bounds_negative_origin() {
        let r = [(-1920, 0, 1920, 1080), (0, 0, 1920, 1080)];
        let (pos, size) = combined_bounds_from_rects(&r).unwrap();
        assert_eq!(pos.x, -1920);
        assert_eq!(size.width, 3840);
    }

    #[test]
    fn materialize_now_playing_with_snapshot() {
        let path = std::path::PathBuf::from("/tmp/x.png");
        let snap = media_art::NowPlayingSnapshot {
            key: "k".into(),
            image_path: path.clone(),
            palette: [[0.; 3]; 3],
            accent_hex: "#000000".into(),
        };
        let s = Settings {
            color_mode: ColorMode::NowPlaying(NowPlayingSource::Spotify),
            ..Default::default()
        };
        let out = materialize_runtime_settings(s, Some(&snap), false);
        assert_eq!(out.color_mode, ColorMode::ImageFile(path));
    }

    #[test]
    fn materialize_now_playing_without_snapshot_falls_back_preset() {
        let s = Settings {
            color_mode: ColorMode::NowPlaying(NowPlayingSource::Automatic),
            ..Default::default()
        };
        let out = materialize_runtime_settings(s, None, false);
        assert_eq!(out.color_mode, ColorMode::Preset(ColorPreset::Original));
    }

    #[test]
    fn materialize_preset_untouched() {
        let s = Settings::default();
        let out = materialize_runtime_settings(s.clone(), None, false);
        assert_eq!(out.color_mode, s.color_mode);
    }

    #[test]
    fn materialize_battery_saver_reduces_rate() {
        let s = Settings::default();
        let out = materialize_runtime_settings(s.clone(), None, true);
        assert!(out.fluid_frame_rate < s.fluid_frame_rate);
        assert!(out.fluid_timestep > s.fluid_timestep);
    }

    #[test]
    fn display_uses_now_playing_linked_span() {
        let mut cfg = AppConfig {
            wallpaper_layout: WallpaperLayout::SpanDisplays,
            monitor_mode: MonitorMode::Linked,
            shared_profile: Settings {
                color_mode: ColorMode::NowPlaying(NowPlayingSource::Spotify),
                ..Default::default()
            },
            ..Default::default()
        };
        cfg.monitors.insert(
            "m1".into(),
            MonitorConfig {
                monitor_id: "m1".into(),
                name_hint: "M1".into(),
                drift_settings: Settings::default(),
            },
        );
        assert!(display_uses_now_playing_colors(&cfg, "m1", true));
    }

    #[test]
    fn display_uses_now_playing_independent_monitor() {
        let mut cfg = AppConfig {
            monitor_mode: MonitorMode::Independent,
            wallpaper_layout: WallpaperLayout::PerMonitor,
            ..Default::default()
        };
        cfg.monitors.insert(
            "mid".into(),
            MonitorConfig {
                monitor_id: "mid".into(),
                name_hint: "Ext".into(),
                drift_settings: Settings {
                    color_mode: ColorMode::NowPlaying(NowPlayingSource::AppleMusic),
                    ..Default::default()
                },
            },
        );
        assert!(display_uses_now_playing_colors(&cfg, "mid", true));
        assert!(!display_uses_now_playing_colors(&cfg, "missing", true));
    }
}
