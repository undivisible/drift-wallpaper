//! Windows wallpaper window implementation.
//!
//! Uses Win32 API to create borderless wallpaper windows that stay behind normal app windows.

use drift_core::FluxRenderer;
use winit::monitor::MonitorHandle;
use winit::window::Window;

pub struct WindowsWallpaperManager;

impl WindowsWallpaperManager {
    pub fn set_desktop_level(window: &Window) -> anyhow::Result<()> {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

        if let Ok(handle) = window.window_handle() {
            if let RawWindowHandle::Win32(h) = handle.as_raw() {
                unsafe {
                    use windows::Win32::Foundation::HWND;
                    use windows::Win32::UI::WindowsAndMessaging::*;

                    let hwnd = HWND(h.hwnd.get() as *mut std::ffi::c_void);
                    let style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
                    let _previous_style = SetWindowLongW(
                        hwnd,
                        GWL_EXSTYLE,
                        (style | WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as i32,
                    );
                    SetWindowPos(
                        hwnd,
                        HWND_BOTTOM,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    )?;
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
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
            if let RawWindowHandle::Win32(h) = handle.as_raw() {
                unsafe {
                    use windows::Win32::Foundation::HWND;
                    use windows::Win32::UI::WindowsAndMessaging::*;

                    let hwnd = HWND(h.hwnd.get() as *mut std::ffi::c_void);
                    SetWindowPos(
                        hwnd,
                        HWND_BOTTOM,
                        position.x,
                        position.y,
                        size.width as i32,
                        size.height as i32,
                        SWP_NOACTIVATE,
                    )?;
                }
            }
        }
        Ok(())
    }

    pub fn snap_to_all_monitors(window: &Window) -> anyhow::Result<()> {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

        if let Ok(handle) = window.window_handle() {
            if let RawWindowHandle::Win32(h) = handle.as_raw() {
                unsafe {
                    use windows::Win32::Foundation::HWND;
                    use windows::Win32::UI::WindowsAndMessaging::*;

                    let hwnd = HWND(h.hwnd.get() as *mut std::ffi::c_void);
                    let position = window.outer_position().unwrap_or_default();
                    let size = window.outer_size();
                    SetWindowPos(
                        hwnd,
                        HWND_BOTTOM,
                        position.x,
                        position.y,
                        size.width as i32,
                        size.height as i32,
                        SWP_NOACTIVATE,
                    )?;
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
