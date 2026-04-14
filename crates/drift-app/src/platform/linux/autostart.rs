//! Linux autostart via XDG .desktop files.
//!
//! Creates a .desktop file in ~/.config/autostart for login item functionality.

use std::path::PathBuf;

use crate::platform::AutostartManager;

pub struct LinuxAutostart;

impl LinuxAutostart {
    fn autostart_file_path() -> PathBuf {
        let config_home = std::env::var("XDG_CONFIG_HOME")
            .unwrap_or_else(|_| format!("{}/.config", std::env::var("HOME").unwrap_or_default()));
        PathBuf::from(config_home)
            .join("autostart")
            .join("drift-wallpaper.desktop")
    }

    fn desktop_entry_template(exe_path: &str) -> String {
        format!(
            r#"[Desktop Entry]
Type=Application
Name=Drift Wallpaper
Comment=Fluid live wallpaper for your desktop
Exec={exe_path} --background
Hidden=false
X-GNOME-Autostart-enabled=true
X-KDE-autostart-after=panel
"#,
            exe_path = exe_path
        )
    }
}

impl AutostartManager for LinuxAutostart {
    fn install() -> anyhow::Result<()> {
        let exe_path = std::env::current_exe()?;
        let exe_str = exe_path.to_string_lossy();

        let dest = Self::autostart_file_path();

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = Self::desktop_entry_template(&exe_str);
        std::fs::write(&dest, content)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&dest)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&dest, perms)?;
        }

        log::info!("Linux autostart installed at {}", dest.display());
        Ok(())
    }

    fn uninstall() -> anyhow::Result<()> {
        let dest = Self::autostart_file_path();

        if dest.exists() {
            std::fs::remove_file(&dest)?;
            log::info!("Linux autostart removed from {}", dest.display());
        }

        Ok(())
    }

    fn is_installed() -> bool {
        Self::autostart_file_path().exists()
    }
}
