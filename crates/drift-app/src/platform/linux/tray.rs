//! Linux system tray implementation.
//!
//! Uses libappindicator (AyatanaAppIndicator) for system tray integration
//! on Linux desktops.

use std::sync::{Arc, Mutex};

use crate::config::AppConfig;
use crate::platform::SystemTray;

pub trait SystemTrayHandle: Send + Sync {
    fn update_menu(&self, config: &AppConfig);
}

pub struct LinuxSystemTray {
    _connection: Option<dbus::Connection>,
}

impl LinuxSystemTray {
    pub fn create_tray(config: Arc<Mutex<AppConfig>>) -> anyhow::Result<Box<dyn SystemTrayHandle>> {
        let connection = dbus::Connection::new_session().ok();

        Ok(Box::new(LinuxSystemTray {
            _connection: connection,
        }))
    }
}

impl SystemTrayHandle for LinuxSystemTray {
    fn update_menu(&self, _config: &AppConfig) {}
}

impl SystemTray for LinuxSystemTray {
    type TrayHandle = Box<dyn SystemTrayHandle>;

    fn create_tray(config: Arc<Mutex<AppConfig>>) -> anyhow::Result<Self::TrayHandle> {
        Self::create_tray(config)
    }
}
