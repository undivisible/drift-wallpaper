//! drift-wallpaper: live Drift fluid wallpaper for macOS.
//!
//! # Architecture
//!
//! ```text
//! main ──► macOS event loop (winit / AppKit)
//!            │
//!            ├─ WallpaperWindow (one per display, at kCGDesktopWindowLevel)
//!            │    └─ DriftRenderer (wgpu + WGSL domain-warped noise shaders)
//!            │
//!            └─ MenuBar (NSStatusItem via objc2)
//!                 ├─ Enable / Disable
//!                 ├─ Colour Preset
//!                 ├─ Extract from Image
//!                 ├─ Start at Login
//!                 └─ Quit
//! ```
//!
//! On non-macOS platforms the binary prints an error message and exits.

mod config;
mod launch_agent;

// macOS-only modules: only compiled when targeting macOS.
#[cfg(target_os = "macos")]
mod menubar;
#[cfg(target_os = "macos")]
mod wallpaper;

// ---------------------------------------------------------------------------
// Non-macOS stub: inform the user that the app is macOS-only.
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("Error: drift-wallpaper requires macOS 14.0 or later.");
    std::process::exit(1);
}

// ---------------------------------------------------------------------------
// macOS entry point
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    use std::sync::{Arc, Mutex};

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("drift-wallpaper starting");

    // Load persisted configuration (or use defaults).
    let config = Arc::new(Mutex::new(config::AppConfig::load()));

    // Run the application.
    run_macos(config)
}

#[cfg(target_os = "macos")]
fn run_macos(config: menubar::SharedConfig) -> anyhow::Result<()> {
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    use objc2_foundation::MainThreadMarker;
    use winit::{
        application::ApplicationHandler,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
        window::{Fullscreen, WindowAttributes, WindowId},
    };

    // Obtain the main-thread marker required by objc2.
    // SAFETY: we are on the main thread (Rust `main` is always the main thread
    //         on macOS).
    let mtm = unsafe { MainThreadMarker::new_unchecked() };

    // Set the app to run as an accessory (no Dock icon, no main menu).
    let app = unsafe { NSApplication::sharedApplication(mtm) };
    unsafe {
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    }

    // Build the status bar menu.  Keep `_status_item` alive for the app lifetime.
    let _status_item = menubar::create_status_item(mtm, Arc::clone(&config));

    // Create the winit event loop.
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    struct App {
        config: menubar::SharedConfig,
        renderers: Vec<drift_core::DriftRenderer>,
    }

    impl ApplicationHandler for App {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if !self.renderers.is_empty() {
                return; // already initialised
            }

            let params = self.config.lock().unwrap().params.clone();

            // Create one fullscreen window per monitor.
            for monitor in event_loop.available_monitors() {
                let size = monitor.size();
                let attrs = WindowAttributes::default()
                    .with_title("drift-wallpaper")
                    .with_fullscreen(Some(Fullscreen::Borderless(Some(monitor.clone()))))
                    .with_decorations(false)
                    .with_transparent(false);

                let window = match event_loop.create_window(attrs) {
                    Ok(w) => std::sync::Arc::new(w),
                    Err(e) => {
                        log::error!("Failed to create window: {e}");
                        continue;
                    }
                };

                // ── macOS: set window level to kCGDesktopWindowLevel ──────
                set_desktop_window_level(&window);

                // Create the wgpu surface tied to this window.
                let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                    backends: wgpu::Backends::all(),
                    ..Default::default()
                });

                // SAFETY: the Arc<Window> keeps the window alive as long as
                // the renderer is alive.
                let surface = unsafe {
                    instance.create_surface_unsafe(
                        wgpu::SurfaceTargetUnsafe::from_window(window.as_ref())
                            .expect("create surface target"),
                    )
                };
                let surface = match surface {
                    Ok(s) => s,
                    Err(e) => {
                        log::error!("Failed to create wgpu surface: {e}");
                        continue;
                    }
                };

                match drift_core::DriftRenderer::new(
                    surface,
                    size.width,
                    size.height,
                    params.clone(),
                ) {
                    Ok(renderer) => self.renderers.push(renderer),
                    Err(e) => log::error!("Failed to create renderer: {e}"),
                }
            }
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            _id: WindowId,
            event: WindowEvent,
        ) {
            match event {
                WindowEvent::Resized(size) => {
                    for r in &mut self.renderers {
                        r.resize(size.width, size.height);
                    }
                }
                WindowEvent::CloseRequested => {
                    event_loop.exit();
                }
                WindowEvent::RedrawRequested => {
                    let enabled = self.config.lock().unwrap().enabled;
                    if enabled {
                        for r in &self.renderers {
                            r.render();
                        }
                    }
                }
                _ => {}
            }
        }

        fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
            // Drive rendering via poll mode – request a redraw every iteration.
            // The OS throttles this to the display refresh rate via Fifo vsync.
        }
    }

    let mut app = App {
        config,
        renderers: Vec::new(),
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}

/// Set the window level to `kCGDesktopWindowLevel` on macOS so the window
/// sits behind all application windows but above the real desktop.
#[cfg(target_os = "macos")]
fn set_desktop_window_level(window: &winit::window::Window) {
    use objc2_app_kit::{NSView, NSWindow, NSWindowCollectionBehavior, NSWindowLevel};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    if let Ok(handle) = window.window_handle() {
        if let RawWindowHandle::AppKit(h) = handle.as_raw() {
            // SAFETY: winit guarantees that `ns_view` is a valid NSView pointer
            // for the lifetime of the window.
            let ns_view = h.ns_view.as_ptr() as *const NSView;
            unsafe {
                if let Some(ns_window) = (*ns_view).window() {
                    // kCGDesktopWindowLevel
                    ns_window.setLevel(NSWindowLevel(-2147483630));
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
