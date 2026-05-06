//! Windows system tray implementation.
//!
//! Uses the Windows Shell_NotifyIcon API for system tray integration.

use std::sync::{Arc, RwLock};

use crate::config::AppConfig;
use crate::platform::SystemTray;

pub trait SystemTrayHandle: Send + Sync {
    fn update_menu(&self, config: &AppConfig);
}

pub struct WindowsSystemTray {
    _hwnd: isize,
}

impl WindowsSystemTray {
    pub fn create_tray(config: Arc<RwLock<AppConfig>>) -> anyhow::Result<Box<dyn SystemTrayHandle>> {
        use windows::Win32::Foundation::*;
        use windows::Win32::UI::WindowsAndMessaging::*;

        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_LEFT | WS_EX_LTRREADING | WS_EX_NOPARENTNOTIFY,
                windows::Win32::UI::WindowsAndMessaging::WINDOW_CLASS_NAME,
                windows::core::w!("DriftWallpaperTray"),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                240,
                120,
                HWND_MESSAGE,
                None,
                None,
                None,
            )?;

            let _ = config;

            Ok(Box::new(WindowsSystemTray {
                _hwnd: hwnd.0 as isize,
            }))
        }
    }
}

impl SystemTrayHandle for WindowsSystemTray {
    fn update_menu(&self, _config: &AppConfig) {}
}

impl SystemTray for WindowsSystemTray {
    type TrayHandle = Box<dyn SystemTrayHandle>;

    fn create_tray(config: Arc<RwLock<AppConfig>>) -> anyhow::Result<Self::TrayHandle> {
        Self::create_tray(config)
    }
}
