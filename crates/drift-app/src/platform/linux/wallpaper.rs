//! Linux wallpaper window implementation.
//!
//! Uses X11 to create borderless override-redirect windows at the desktop level.

use winit::monitor::MonitorHandle;
use winit::window::Window;

use drift_core::FluxRenderer;

pub struct LinuxWallpaperManager;

impl LinuxWallpaperManager {
    pub fn set_desktop_level(window: &Window) -> anyhow::Result<()> {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

        if let Ok(handle) = window.window_handle() {
            if let RawWindowHandle::Xlib(h) = handle.as_raw() {
                unsafe {
                    use x11::xlib::*;

                    let display = h.display as *mut Display;
                    let window = xlib::Window::from(h.window as *mut std::ffi::c_void);

                    if display.is_null() || window == 0 {
                        return Ok(());
                    }

                    let root = XDefaultRootWindow(display);
                    XReparentWindow(display, window, root, 0, 0);

                    let mut attributes: XSetWindowAttributes = std::mem::zeroed();
                    attributes.event_mask = ExposureMask | StructureNotifyMask;
                    attributes.cursor = None;

                    XChangeWindowAttributes(display, window, CWEventMask | CWCursor, &attributes);

                    XLowerWindow(display, window);
                    XFlush(display);
                }
            }
        }
        Ok(())
    }

    pub fn snap_to_monitor(window: &Window, monitor: &MonitorHandle) -> anyhow::Result<()> {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

        let position = monitor.position();
        let size = monitor.size();

        if let Ok(handle) = window.window_handle() {
            if let RawWindowHandle::Xlib(h) = handle.as_raw() {
                unsafe {
                    use x11::xlib::*;

                    let display = h.display as *mut Display;
                    let window = xlib::Window::from(h.window as *mut std::ffi::c_void);

                    if display.is_null() || window == 0 {
                        return Ok(());
                    }

                    let root = XDefaultRootWindow(display);

                    XReparentWindow(display, window, root, position.x, position.y);
                    XResizeWindow(display, window, size.width, size.height);
                    XLowerWindow(display, window);
                    XFlush(display);
                }
            }
        }
        Ok(())
    }

    pub fn snap_to_all_monitors(window: &Window) -> anyhow::Result<()> {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

        if let Ok(handle) = window.window_handle() {
            if let RawWindowHandle::Xlib(h) = handle.as_raw() {
                unsafe {
                    use x11::xlib::*;
                    use x11::xrandr::*;

                    let display = h.display as *mut Display;
                    let window = xlib::Window::from(h.window as *mut std::ffi::c_void);

                    if display.is_null() || window == 0 {
                        return Ok(());
                    }

                    let root = XDefaultRootWindow(display);
                    let resources = XRRGetScreenResources(display, root);

                    if resources.is_null() {
                        let position = window
                            .primary_monitor()
                            .map(|m| m.position())
                            .unwrap_or_else(|| (0, 0).into());
                        let size = window
                            .primary_monitor()
                            .map(|m| m.size())
                            .unwrap_or_else(|| (1920, 1080).into());

                        XMoveResizeWindow(
                            display,
                            window,
                            position.x,
                            position.y,
                            size.width,
                            size.height,
                        );
                        return Ok(());
                    }

                    let noutputs = (*resources).noutput;
                    let outputs = (*resources).outputs;

                    let mut min_x = i32::MAX;
                    let mut min_y = i32::MAX;
                    let mut max_x = i32::MIN;
                    let mut max_y = i32::MIN;

                    for i in 0..noutputs {
                        let output = *outputs.offset(i as isize);
                        let info = XRRGetOutputInfo(display, resources, output);

                        if !info.is_null() && (*info).crtc != 0 {
                            let crtc = XRRGetCrtcInfo(display, resources, (*info).crtc);
                            if !crtc.is_null() {
                                let rect = (*crtc);
                                min_x = min_x.min(rect.x as i32);
                                min_y = min_y.min(rect.y as i32);
                                max_x = max_x.max((rect.x + rect.width) as i32);
                                max_y = max_y.max((rect.y + rect.height) as i32);
                                XRRFreeCrtcInfo(crtc);
                            }
                            XRRFreeOutputInfo(info);
                        }
                    }

                    XRRFreeScreenResources(resources);

                    if min_x != i32::MAX {
                        let width = (max_x - min_x) as c_uint;
                        let height = (max_y - min_y) as c_uint;

                        XReparentWindow(display, window, root, min_x, min_y);
                        XResizeWindow(display, window, width, height);
                        XLowerWindow(display, window);
                    }

                    XFlush(display);
                }
            }
        }
        Ok(())
    }

    pub fn sync_renderer_size(window: &Window, renderer: &mut FluxRenderer) -> anyhow::Result<()> {
        let physical = window.inner_size();
        let logical = physical.to_logical::<u32>(window.scale_factor());
        renderer.resize(
            logical.width,
            logical.height,
            physical.width,
            physical.height,
        );
        Ok(())
    }
}
