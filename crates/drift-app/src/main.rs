mod cli;
mod color_picker;
mod crepus_interactive;
mod crepus_settings_render;
mod config;
mod launch_agent;
mod media_art;
mod now_playing;
mod ui;

#[cfg(target_os = "macos")]
mod menubar;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
        now_playing_key: Option<String>,
        now_playing_source: Option<drift_core::NowPlayingSource>,
        now_playing_snapshot: Option<media_art::NowPlayingSnapshot>,
        now_playing_controller: Option<now_playing::NowPlayingController>,
        now_playing_updates_rx: std::sync::mpsc::Receiver<now_playing::NowPlayingUpdate>,
    }

    fn sync_windows(app: &mut App, event_loop: &ActiveEventLoop) {
        let (desired_signature, window_specs, monitor_handles) = build_window_specs(app, event_loop);

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

            let settings = app
                .config
                .lock()
                .map(|cfg| {
                    if app.wallpaper_mode
                        && cfg.wallpaper_layout == WallpaperLayout::SpanDisplays
                        && !cfg.monitors.is_empty()
                    {
                        cfg.wallpaper_profile().clone()
                    } else {
                        cfg.settings_for_monitor(&monitor.id)
                    }
                })
                .unwrap_or_default();

            let settings =
                materialize_runtime_settings(settings, app.now_playing_snapshot.as_ref());

            match create_renderer(Arc::clone(&window), settings.clone()) {
                Ok(mut renderer) => {
                    #[cfg(target_os = "macos")]
                    if app.wallpaper_mode
                        && wallpaper_layout == WallpaperLayout::SpanDisplays
                        && i == 0
                    {
                        // AppKit may resize the window after `setFrame`; sync wgpu to the real
                        // backing size so the fluid sim covers all displays (not just one).
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
                if let Ok(latest) = AppConfig::try_load() {
                    if let Ok(mut current) = self.config.lock() {
                        *current = latest;
                    }
                }
                self.last_config_refresh = Instant::now();
            }

            // Single config snapshot for the entire frame — avoids repeated mutex churn.
            let cfg = match self.config.lock() {
                Ok(g) => g.clone(),
                Err(_) => return,
            };

            // Drain now-playing updates from the worker thread. Do not filter by
            // `update.source`: the snapshot is only applied when color mode is NowPlaying;
            // filtering caused missed updates when config/source timing briefly disagreed.
            let current_source = cfg.wallpaper_profile().color_mode.now_playing_source();
            while let Ok(update) = self.now_playing_updates_rx.try_recv() {
                self.now_playing_source = update.source;
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

            // Detect now-playing source change in config and force a worker refresh.
            if self.now_playing_source != current_source {
                self.now_playing_key = None;
                self.now_playing_source = current_source;
                self.now_playing_snapshot = None;
                if let Some(controller) = &self.now_playing_controller {
                    controller.request_refresh();
                }
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

                settings =
                    materialize_runtime_settings(settings, self.now_playing_snapshot.as_ref());

                let uses_now_playing = display_uses_now_playing_colors(
                    &cfg,
                    &display.monitor_id,
                    self.wallpaper_mode,
                );
                let np_key = uses_now_playing.then(|| self.now_playing_key.clone()).flatten();
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
                let fps = cfg.wallpaper_profile().fluid_frame_rate.clamp(1.0, 240.0);
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
        now_playing_key: None,
        now_playing_source: None,
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
    let first = monitors.first()?;
    let mut min_x = first.position().x;
    let mut min_y = first.position().y;
    let mut max_x = first.position().x + first.size().width as i32;
    let mut max_y = first.position().y + first.size().height as i32;

    for monitor in monitors.iter().skip(1) {
        let position = monitor.position();
        let size = monitor.size();
        min_x = min_x.min(position.x);
        min_y = min_y.min(position.y);
        max_x = max_x.max(position.x + size.width as i32);
        max_y = max_y.max(position.y + size.height as i32);
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
) -> Settings {
    if matches!(settings.color_mode, ColorMode::NowPlaying(_)) {
        if let Some(snapshot) = snapshot {
            settings.color_mode = ColorMode::ImageFile(snapshot.image_path.clone());
        } else {
            settings.color_mode = ColorMode::Preset(drift_core::ColorPreset::Original);
        }
    }

    settings
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

/// Winit’s initial `position` / `inner_size` can disagree with AppKit for secondary displays.
/// Snap the wallpaper `NSWindow` to the `NSScreen` frame (same approach as `wallpaper.rs`).
///
/// If we can’t obtain the `NSScreen` for this monitor we intentionally skip the frame snap
/// rather than falling back to the main screen — falling back would overlay two windows on the
/// primary monitor and leave the secondary monitor uncovered.
#[cfg(target_os = "macos")]
fn macos_snap_wallpaper_window_to_monitor(
    window: &winit::window::Window,
    monitor: &MonitorHandle,
) {
    use objc2_app_kit::{NSView, NSScreen};
    use objc2_foundation::NSRect;
    use winit::platform::macos::MonitorHandleExtMacOS;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    // Only snap if we can get the exact NSScreen; otherwise leave winit’s placement in place.
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

/// One window spanning all displays: match the union of every `NSScreen.frame` in global coordinates.
#[cfg(target_os = "macos")]
fn macos_snap_wallpaper_window_to_union_of_screens(window: &winit::window::Window) {
    use objc2_app_kit::{NSView, NSScreen};
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
                            // `true` asks AppKit to update display wiring promptly (important when
                            // the window spans multiple `NSScreen`s).
                            ns_window.setFrame_display(union, true);
                            ns_window.orderFrontRegardless();
                        }
                    }
        }
    }
}
