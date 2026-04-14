//! Linux color picker via zenity or yad.
//!
//! Tries to use zenity for a GTK-based color picker dialog.

use std::process::Command;

use crate::platform::NativeColorPicker;

pub struct LinuxColorPicker;

impl LinuxColorPicker {
    pub fn pick_color(initial: Option<[u8; 3]>) -> anyhow::Result<Option<[u8; 3]>> {
        if let Some(color) = Self::pick_color_zenity(initial)? {
            return Ok(Some(color));
        }

        if let Some(color) = Self::pick_color_yad(initial)? {
            return Ok(Some(color));
        }

        Ok(initial)
    }

    fn pick_color_zenity(initial: Option<[u8; 3]>) -> anyhow::Result<Option<[u8; 3]>> {
        let mut cmd = Command::new("zenity");
        cmd.arg("--color-selection");
        cmd.arg("--show-palette");

        if let Some([r, g, b]) = initial {
            cmd.arg(format!("#{r:02x}{g:02x}{b:02x}"));
        }

        let output = cmd.output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let color_str = stdout.trim();

            if color_str.starts_with('#') && color_str.len() == 7 {
                let r = u8::from_str_radix(&color_str[1..3], 16)?;
                let g = u8::from_str_radix(&color_str[3..5], 16)?;
                let b = u8::from_str_radix(&color_str[5..7], 16)?;
                return Ok(Some([r, g, b]));
            }
        }

        Ok(None)
    }

    fn pick_color_yad(initial: Option<[u8; 3]>) -> anyhow::Result<Option<[u8; 3]>> {
        let mut cmd = Command::new("yad");
        cmd.arg("--color");
        cmd.arg("--show-palette");

        if let Some([r, g, b]) = initial {
            cmd.arg(format!("#{r:02x}{g:02x}{b:02x}"));
        }

        let output = cmd.output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let color_str = stdout.trim();

            if color_str.starts_with('#') && color_str.len() == 7 {
                let r = u8::from_str_radix(&color_str[1..3], 16)?;
                let g = u8::from_str_radix(&color_str[3..5], 16)?;
                let b = u8::from_str_radix(&color_str[5..7], 16)?;
                return Ok(Some([r, g, b]));
            }
        }

        Ok(None)
    }
}

impl NativeColorPicker for LinuxColorPicker {
    fn pick_color(initial: Option<[u8; 3]>) -> anyhow::Result<Option<[u8; 3]>> {
        Self::pick_color(initial)
    }
}
