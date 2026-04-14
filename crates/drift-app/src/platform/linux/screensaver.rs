//! Linux screensaver integration.
//!
//! Supports integration with:
//!   - xscreensaver (via .desktop file)
//!   - gnome-screensaver
//!   - kde-screensaver
//!   - xdg-screensaver protocol

use std::path::PathBuf;

use crate::platform::{ScreensaverMode, ScreensaverRunner};

pub struct LinuxScreensaverRunner;

impl LinuxScreensaverRunner {
    fn find_screensaver_commands() -> Vec<(&'static str, &'static str)> {
        vec![
            ("xscreensaver", "xscreensaver-command -activate"),
            ("gnome-screensaver", "gnome-screensaver-command -a"),
            ("cinnamon-screensaver", "cinnamon-screensaver-command -a"),
            ("mate-screensaver", "mate-screensaver-command -a"),
        ]
    }
}

impl ScreensaverRunner for LinuxScreensaverRunner {
    fn parse_screensaver_args() -> ScreensaverMode {
        let args: Vec<String> = std::env::args().collect();

        for arg in &args[1..] {
            match arg.as_str() {
                "--screensaver" | "-s" => return ScreensaverMode::Fullscreen,
                "--configure" | "-c" => return ScreensaverMode::Configure,
                "--preview" | "-p" => {
                    if let Some(idx) = args.iter().position(|a| a == "--preview" || a == "-p") {
                        if let Some(id_str) = args.get(idx + 1) {
                            if let Ok(hwnd) = id_str.parse::<usize>() {
                                return ScreensaverMode::Preview { parent_hwnd: hwnd };
                            }
                        }
                    }
                    return ScreensaverMode::Preview { parent_hwnd: 0 };
                }
                _ => {}
            }
        }

        ScreensaverMode::None
    }

    fn configure_as_screensaver() -> anyhow::Result<()> {
        let exe_path = std::env::current_exe()?;
        let exe_str = exe_path.to_string_lossy();

        let config_home = std::env::var("XDG_CONFIG_HOME")
            .unwrap_or_else(|_| format!("{}/.config", std::env::var("HOME").unwrap_or_default()));

        let screensavers_dir = PathBuf::from(&config_home).join("xscreensaver");

        std::fs::create_dir_all(&screensavers_dir)?;

        let desktop_content = format!(
            r#"[Desktop Entry]
Name=Drift Wallpaper
Comment=Fluid live wallpaper screensaver
Exec={exe_path} --screensaver
StartupNotify=false
Terminal=false
Type=Application
Categories=Graphics;
"#,
            exe_path = exe_str
        );

        let desktop_path = screensavers_dir.join("drift-wallpaper.desktop");
        std::fs::write(&desktop_path, desktop_content)?;

        let _ = std::process::Command::new("xdg-screensaver")
            .arg("install")
            .arg(&exe_path)
            .output();

        println!("Screensaver installation:");
        println!("1. Desktop file created at: {}", desktop_path.display());
        println!("2. You may need to restart your screensaver daemon");
        println!("3. Or run: xdg-screensaver install {}", exe_path.display());

        Ok(())
    }
}
