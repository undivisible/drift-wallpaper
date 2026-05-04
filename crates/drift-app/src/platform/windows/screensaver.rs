//! Windows screensaver implementation.
//!
//! Windows screensavers are .scr files (renamed executables) that support
//! command-line arguments:
//!   /s - Run as screensaver (fullscreen)
//!   /p <hwnd> - Preview in a window
//!   /c - Configuration dialog

use crate::platform::{ScreensaverMode, ScreensaverRunner};

pub struct WindowsScreensaverRunner;

impl ScreensaverRunner for WindowsScreensaverRunner {
    fn parse_screensaver_args() -> ScreensaverMode {
        let args: Vec<String> = std::env::args().collect();

        if args.len() < 2 {
            return ScreensaverMode::None;
        }

        match args[1].to_lowercase().as_str() {
            "/s" => ScreensaverMode::Fullscreen,
            "/c" => ScreensaverMode::Configure,
            "/p" if args.len() >= 3 => {
                let hwnd = args[2].parse::<usize>().unwrap_or(0);
                ScreensaverMode::Preview { parent_hwnd: hwnd }
            }
            _ => ScreensaverMode::None,
        }
    }

    fn configure_as_screensaver() -> anyhow::Result<()> {
        let exe_path = std::env::current_exe()?;
        let exe_dir = exe_path.parent().unwrap_or(std::path::Path::new("."));

        let scr_path = exe_dir.join("drift-wallpaper.scr");

        std::fs::copy(&exe_path, &scr_path)?;

        println!("To install as screensaver:");
        println!(
            "1. Copy {} to C:\\Windows\\System32\\drift-wallpaper.scr",
            scr_path.display()
        );
        println!("2. Select 'Drift Wallpaper' in Windows screensaver settings");

        Ok(())
    }
}
