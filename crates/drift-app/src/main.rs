//! drift-wallpaper: live drift-style wallpaper renderer.
//!
//! # Architecture
//!
//! ```text
//! Default (no CLI flags): live wallpaper on the desktop + menu bar (macOS).
//! `drift-wallpaper --settings` opens only the control panel.
//!
//! main ──► event loop (winit / AppKit on macOS)
//!            │
//!            ├─ wallpaper windows (macOS) or a preview window (Linux/Windows)
//!            │    └─ DriftRenderer (wgpu + WGSL domain-warped noise shaders)
//!            │
//!            └─ MenuBar (NSStatusItem via objc2)
//!                 ├─ Enable / Disable
//!                 ├─ Colour Preset
//!                 ├─ Extract from Image
//!                 ├─ Start at Login
//!                 └─ Quit
//! ```

mod cli;
mod config;
mod launch_agent;
mod ui;

// macOS-only modules: only compiled when targeting macOS.
#[cfg(target_os = "macos")]
mod menubar;

fn init_logging() {
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    // wgpu can log `Device::maintain: waiting for submission` at INFO every frame.
    builder.filter_module("wgpu_core::device::resource", log::LevelFilter::Warn);
    builder.filter_module("wgpu_hal", log::LevelFilter::Warn);
    builder.init();
}

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

#[cfg(not(target_os = "macos"))]
fn main() -> Result<()> {
    init_logging();
    let mut config = config::AppConfig::load();
    let action = cli::apply_cli_args(&mut config)?;
    match action {
        cli::StartupAction::Exit => Ok(()),
        cli::StartupAction::Run { mode } => match mode {
            cli::RunMode::Ui => ui::run_ui(config),
            cli::RunMode::Background => {
                log::info!("drift-wallpaper starting in desktop window mode");
                let config = Arc::new(Mutex::new(config));
                run_app(config, true)
            }
            cli::RunMode::Preview => {
                log::info!("drift-wallpaper starting in preview mode");
                let config = Arc::new(Mutex::new(config));
                run_app(config, false)
            }
        },
    }
}

#[cfg(target_os = "macos")]
fn main() -> Result<()> {
    init_logging();
    let mut config = config::AppConfig::load();
    let action = cli::apply_cli_args(&mut config)?;
    match action {
        cli::StartupAction::Exit => Ok(()),
        cli::StartupAction::Run { mode } => match mode {
            cli::RunMode::Ui => ui::run_ui(config),
            cli::RunMode::Background => {
                log::info!("drift-wallpaper starting");
                let config = Arc::new(Mutex::new(config));
                run_macos(config)
            }
            cli::RunMode::Preview => {
                log::info!("drift-wallpaper starting in preview mode");
                let config = Arc::new(Mutex::new(config));
                run_app(config, false)
            }
        },
    }
}

#[cfg(target_os = "macos")]
fn run_macos(config: Arc<Mutex<config::AppConfig>>) -> Result<()> {
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    use objc2_foundation::MainThreadMarker;

    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let _status_item = menubar::create_status_item(mtm, Arc::clone(&config));
    run_app(config, true)
}

struct DisplayWindow {
    id: WindowId,
    window: Arc<Window>,
    renderer: drift_core::DriftRenderer,
}

fn run_app(config: Arc<Mutex<config::AppConfig>>, wallpaper_mode: bool) -> Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    struct App {
        config: Arc<Mutex<config::AppConfig>>,
        wallpaper_mode: bool,
        windows: Vec<DisplayWindow>,
        last_config_refresh: Instant,
    }

    impl ApplicationHandler for App {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if !self.windows.is_empty() {
                return;
            }

            let params = self.config.lock().unwrap().params.clone();
            let window_specs = if self.wallpaper_mode {
                let mut monitors: Vec<_> = event_loop.available_monitors().collect();
                if monitors.is_empty() {
                    if let Some(primary) = event_loop.primary_monitor() {
                        monitors.push(primary);
                    }
                }
                monitors
                    .into_iter()
                    .map(|monitor| {
                        let size = monitor.size();
                        let position = monitor.position();
                        WindowAttributes::default()
                            .with_title("drift-wallpaper")
                            .with_decorations(false)
                            .with_transparent(false)
                            .with_resizable(false)
                            .with_position(position)
                            .with_inner_size(size)
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![WindowAttributes::default()
                    .with_title("Drift Wallpaper")
                    .with_inner_size(LogicalSize::new(1280.0, 720.0))]
            };

            for attrs in window_specs {
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

                let size = window.inner_size();
                match create_renderer(Arc::clone(&window), params.clone()) {
                    Ok(renderer) => self.windows.push(DisplayWindow {
                        id: window.id(),
                        window,
                        renderer,
                    }),
                    Err(error) => {
                        log::error!(
                            "Failed to create renderer for {}x{} window: {error}",
                            size.width,
                            size.height
                        );
                    }
                }
            }
        }

        fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
            match event {
                WindowEvent::Resized(size) => {
                    if let Some(display) = self.windows.iter_mut().find(|display| display.id == id)
                    {
                        display.renderer.resize(size.width, size.height);
                    }
                }
                WindowEvent::CloseRequested => {
                    self.windows.retain(|display| display.id != id);
                    if self.windows.is_empty() {
                        event_loop.exit();
                    }
                }
                WindowEvent::RedrawRequested => {
                    let (enabled, params) = {
                        let config = self.config.lock().unwrap();
                        (config.enabled, config.params.clone())
                    };
                    if enabled {
                        if let Some(display) =
                            self.windows.iter_mut().find(|display| display.id == id)
                        {
                            display.renderer.set_params(params);
                            if !display.renderer.render() {
                                let size = display.window.inner_size();
                                display.renderer.resize(size.width, size.height);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
            if self.last_config_refresh.elapsed() >= Duration::from_millis(500) {
                if let Ok(latest) = config::AppConfig::try_load() {
                    if let Ok(mut current) = self.config.lock() {
                        if *current != latest {
                            *current = latest;
                        }
                    }
                }
                self.last_config_refresh = Instant::now();
            }
            // Avoid requesting frames while paused: wgpu otherwise keeps "waiting for submission"
            // on redraws that never submit work.
            if self.config.lock().unwrap().enabled {
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

fn create_renderer(
    window: Arc<Window>,
    params: drift_core::DriftParams,
) -> Result<drift_core::DriftRenderer> {
    let size = window.inner_size();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
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

    drift_core::DriftRenderer::new(instance, surface, size.width, size.height, params)
}

/// Set the window level to `kCGDesktopWindowLevel` on macOS so the window
/// sits in the desktop layer with the static wallpaper (see `CGWindowLevel.h`:
/// `kCGDesktopWindowLevel = kCGMinimumWindowLevel + 20`, i.e. `INT32_MIN + 5 + 20`).
///
/// Previously this used `-2147483630`, which sits *below* that level and can
/// leave the renderer invisible behind the system wallpaper.
#[cfg(target_os = "macos")]
fn set_desktop_window_level(window: &winit::window::Window) {
    use objc2_app_kit::{NSView, NSWindowCollectionBehavior, NSWindowLevel};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    /// `kCGDesktopWindowLevel` (`CGWindowLevel.h`: `INT32_MIN + 5 + 20`).
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
