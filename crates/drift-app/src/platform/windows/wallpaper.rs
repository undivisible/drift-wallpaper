//! Windows wallpaper window implementation.
//!
//! Uses Win32 API to create borderless windows at the desktop level
//! that ignore mouse events.

use winit::monitor::MonitorHandle;
use winit::window::Window;

use drift_core::FluxRenderer;

pub struct WindowsWallpaperManager;

impl WindowsWallpaperManager {
    pub fn set_desktop_level(window: &Window) -> anyhow::Result<()> {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

        if let Ok(handle) = window.window_handle() {
            if let RawWindowHandle::Win32(h) = handle.as_raw() {
                unsafe {
                    use windows::Win32::UI::WindowsAndMessaging::*;

                    let hwnd = windows::Win32::HWND(h.hwnd.get() as *mut std::ffi::c_void);

                    let desktop_hwnd = GetDesktopWindow();

                    SetWindowPos(
                        hwnd,
                        desktop_hwnd,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    )?;

                    let style: u32 = GetWindowLongW(hwnd, GWL_EXSTYLE).into();
                    SetWindowLongW(
                        hwnd,
                        GWL_EXSTYLE,
                        windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(
                            style | WS_EX_NOACTIVATE.0 as u32 | WS_EX_TOOLWINDOW.0 as u32,
                        ),
                    )?;

                    SetWindowPos(
                        hwnd,
                        HWND_BOTTOM,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_HIDEWINDOW,
                    )?;

                    ShowWindow(hwnd, SW_SHOWNOACTIVATE)?;
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
                    use windows::Win32::UI::WindowsAndMessaging::*;

                    let hwnd = windows::Win32::HWND(h.hwnd.get() as *mut std::ffi::c_void);

                    SetWindowPos(
                        hwnd,
                        HWND_TOPMOST,
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
                    use windows::Win32::Graphics::Gdi::*;
                    use windows::Win32::UI::WindowsAndMessaging::*;

                    let hwnd = windows::Win32::HWND(h.hwnd.get() as *mut std::ffi::c_void);

                    let mut min_x = i32::MAX;
                    let mut min_y = i32::MAX;
                    let mut max_x = i32::MIN;
                    let mut max_y = i32::MIN;

                    EnumDisplayMonitors(
                        HDC::default(),
                        None,
                        Some(monitor_enum_callback),
                        std::ptr::addr_of_mut!(min_x) as isize,
                    )?;

                    let monitor = window
                        .primary_monitor()
                        .ok_or_else(|| anyhow::anyhow!("No primary monitor"))?;
                    let position = monitor.position();
                    let size = monitor.size();

                    SetWindowPos(
                        hwnd,
                        HWND_TOPMOST,
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

unsafe extern "system" fn monitor_enum_callback(
    hmonitor: windows::Win32::UI::WindowsAndMessaging::HMONITOR,
    _hdc: windows::Win32::Graphics::Gdi::HDC,
    _rect: *mut windows::Win32::Foundation::RECT,
    lparam: isize,
) -> windows::Win32::Foundation::BOOL {
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::UI::WindowsAndMessaging::*;

    let min_x = &mut *(lparam as *mut i32);
    let min_y = &mut *((lparam + 8) as *mut i32);
    let max_x = &mut *((lparam + 16) as *mut i32);
    let max_y = &mut *((lparam + 24) as *mut i32);

    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

    if GetMonitorInfoW(hmonitor, std::mem::cast_mut(&mut info)) {
        let rect = info.monitorInfo.rcMonitor;
        *min_x = min_x.min(rect.left);
        *min_y = min_y.min(rect.top);
        *max_x = max_x.max(rect.right);
        *max_y = max_y.max(rect.bottom);
    }

    BOOL(1)
}
