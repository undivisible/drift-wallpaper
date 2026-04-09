mod cli;
mod config;
mod launch_agent;
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

use crate::config::{AppConfig, DiscoveredMonitor};

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
}

fn run_app(config: Arc<Mutex<AppConfig>>, wallpaper_mode: bool) -> Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    struct App {
        config: Arc<Mutex<AppConfig>>,
        wallpaper_mode: bool,
        windows: Vec<DisplayWindow>,
        last_config_refresh: Instant,
    }

    impl ApplicationHandler for App {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if !self.windows.is_empty() {
                return;
            }

            let window_specs = if self.wallpaper_mode {
                let mut monitors: Vec<_> = event_loop.available_monitors().collect();
                if monitors.is_empty() {
                    if let Some(primary) = event_loop.primary_monitor() {
                        monitors.push(primary);
                    }
                }
                let discovered: Vec<_> = monitors.iter().map(discover_monitor).collect();
                if let Ok(mut cfg) = self.config.lock() {
                    if cfg.ensure_monitors(&discovered) {
                        let _ = cfg.save();
                    }
                }

                monitors
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
                    .collect::<Vec<_>>()
            } else {
                vec![(
                    DiscoveredMonitor {
                        id: "preview".to_string(),
                        name_hint: "Preview".to_string(),
                    },
                    WindowAttributes::default()
                        .with_title("Flux Wallpaper Preview")
                        .with_inner_size(LogicalSize::new(1280.0, 720.0)),
                )]
            };

            for (monitor, attrs) in window_specs {
                let window = match event_loop.create_window(attrs) {
                    Ok(window) => Arc::new(window),
                    Err(error) => {
                        log::error!("Failed to create window: {error}");
                        continue;
                    }
                };

                #[cfg(target_os = "macos")]
                if self.wallpaper_mode {
                    set_desktop_window_level(window.as_ref());
                }

                let settings = self
                    .config
                    .lock()
                    .map(|cfg| cfg.settings_for_monitor(&monitor.id))
                    .unwrap_or_default();

                match create_renderer(Arc::clone(&window), settings.clone()) {
                    Ok(renderer) => self.windows.push(DisplayWindow {
                        id: window.id(),
                        window,
                        monitor_id: monitor.id,
                        renderer,
                        applied_settings: settings,
                    }),
                    Err(error) => log::error!("Failed to create renderer: {error}"),
                }
            }
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
                        let settings = self
                            .config
                            .lock()
                            .map(|cfg| cfg.settings_for_monitor(&display.monitor_id))
                            .unwrap_or_default();
                        if display.applied_settings != settings {
                            match display.renderer.set_settings(settings.clone()) {
                                Ok(()) => display.applied_settings = settings,
                                Err(error) => log::warn!("update renderer settings: {error}"),
                            }
                        }
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

        fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
            if self.last_config_refresh.elapsed() >= Duration::from_millis(500) {
                if let Ok(latest) = AppConfig::try_load() {
                    if let Ok(mut current) = self.config.lock() {
                        *current = latest;
                    }
                }
                self.last_config_refresh = Instant::now();
            }

            if self.config.lock().map(|cfg| cfg.enabled).unwrap_or(true) {
                for display in &self.windows {
                    display.window.request_redraw();
                }
            }
        }
    }

    let mut app = App {
        config,
        wallpaper_mode,
        windows: Vec::new(),
        last_config_refresh: Instant::now(),
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}

fn create_renderer(window: Arc<Window>, settings: Settings) -> Result<FluxRenderer> {
    let physical = window.inner_size();
    let logical = physical.to_logical::<u32>(window.scale_factor());
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });

    let surface = unsafe {
        instance.create_surface_unsafe(
            wgpu::SurfaceTargetUnsafe::from_window(window.as_ref())
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
