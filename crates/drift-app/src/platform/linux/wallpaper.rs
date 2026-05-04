//! Linux wallpaper window implementation.
//!
//! Uses X11 to create borderless override-redirect windows at the desktop level.

use winit::monitor::MonitorHandle;
use winit::window::Window;

use drift_core::FluxRenderer;

pub struct LinuxWallpaperManager;

impl LinuxWallpaperManager {
    pub fn set_desktop_level(window: &Window) -> anyhow::Result<()> {
        use std::ffi::c_uint;
        use winit::raw_window_handle::{
            HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
        };

        if let (Ok(display_handle), Ok(window_handle)) =
            (window.display_handle(), window.window_handle())
        {
            if let (RawDisplayHandle::Xlib(display_handle), RawWindowHandle::Xlib(window_handle)) =
                (display_handle.as_raw(), window_handle.as_raw())
            {
                unsafe {
                    use x11::xlib::*;

                    let Some(display) = display_handle.display else {
                        return Ok(());
                    };
                    let display = display.as_ptr() as *mut Display;
                    let xwindow = window_handle.window;

                    if display.is_null() || xwindow == 0 {
                        return Ok(());
                    }

                    let root = XDefaultRootWindow(display);
                    XReparentWindow(display, xwindow, root, 0, 0);

                    let mut attributes: XSetWindowAttributes = std::mem::zeroed();
                    attributes.event_mask = ExposureMask | StructureNotifyMask;
                    attributes.cursor = 0;

                    XChangeWindowAttributes(
                        display,
                        xwindow,
                        (CWEventMask | CWCursor) as c_uint as _,
                        &mut attributes,
                    );

                    XLowerWindow(display, xwindow);
                    XFlush(display);
                }
            }
        }
        Ok(())
    }

    pub fn snap_to_monitor(window: &Window, monitor: &MonitorHandle) -> anyhow::Result<()> {
        use winit::raw_window_handle::{
            HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
        };

        let position = monitor.position();
        let size = monitor.size();

        if let (Ok(display_handle), Ok(window_handle)) =
            (window.display_handle(), window.window_handle())
        {
            if let (RawDisplayHandle::Xlib(display_handle), RawWindowHandle::Xlib(window_handle)) =
                (display_handle.as_raw(), window_handle.as_raw())
            {
                unsafe {
                    use x11::xlib::*;

                    let Some(display) = display_handle.display else {
                        return Ok(());
                    };
                    let display = display.as_ptr() as *mut Display;
                    let xwindow = window_handle.window;

                    if display.is_null() || xwindow == 0 {
                        return Ok(());
                    }

                    let root = XDefaultRootWindow(display);

                    XReparentWindow(display, xwindow, root, position.x, position.y);
                    XResizeWindow(display, xwindow, size.width, size.height);
                    XLowerWindow(display, xwindow);
                    XFlush(display);
                }
            }
        }
        Ok(())
    }

    pub fn snap_to_all_monitors(window: &Window) -> anyhow::Result<()> {
        use std::ffi::c_uint;
        use winit::raw_window_handle::{
            HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
        };

        if let (Ok(display_handle), Ok(window_handle)) =
            (window.display_handle(), window.window_handle())
        {
            if let (RawDisplayHandle::Xlib(display_handle), RawWindowHandle::Xlib(window_handle)) =
                (display_handle.as_raw(), window_handle.as_raw())
            {
                unsafe {
                    use x11::xlib::*;
                    use x11::xrandr::*;

                    let Some(display) = display_handle.display else {
                        return Ok(());
                    };
                    let display = display.as_ptr() as *mut Display;
                    let xwindow = window_handle.window;

                    if display.is_null() || xwindow == 0 {
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
                            xwindow,
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
                                let rect = *crtc;
                                min_x = min_x.min(rect.x as i32);
                                min_y = min_y.min(rect.y as i32);
                                max_x = max_x.max(rect.x.saturating_add(rect.width as i32));
                                max_y = max_y.max(rect.y.saturating_add(rect.height as i32));
                                XRRFreeCrtcInfo(crtc);
                            }
                            XRRFreeOutputInfo(info);
                        }
                    }

                    XRRFreeScreenResources(resources);

                    if min_x != i32::MAX {
                        let width = (max_x - min_x) as c_uint;
                        let height = (max_y - min_y) as c_uint;

                        XReparentWindow(display, xwindow, root, min_x, min_y);
                        XResizeWindow(display, xwindow, width, height);
                        XLowerWindow(display, xwindow);
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
