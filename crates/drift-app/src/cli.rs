use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use drift_core::{ColorMode, ColorPreset};

use crate::config::AppConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    Ui,
    Background,
    Preview,
}

pub enum StartupAction {
    Run { mode: RunMode },
    Exit,
}

pub fn apply_cli_args(config: &mut AppConfig) -> Result<StartupAction> {
    let mut args = std::env::args().skip(1).peekable();
    let mut mode = RunMode::Background;
    let mut changed = false;
    let mut print_config = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                print_help();
                return Ok(StartupAction::Exit);
            }
            "--settings" | "--ui" => mode = RunMode::Ui,
            "--background" => mode = RunMode::Background,
            "--preview" => mode = RunMode::Preview,
            "--preset" => {
                let value = next_value(&mut args, "--preset")?;
                apply_named_preset(config, &value)?;
                changed = true;
            }
            "--image" | "--screenshot" => {
                let value = next_value(&mut args, arg.as_str())?;
                apply_image_color_mode(config, Path::new(&value))?;
                changed = true;
            }
            "--wallpaper-image" => {
                apply_current_wallpaper_color_mode(config)?;
                changed = true;
            }
            "--wallpaper-screenshot" => {
                apply_wallpaper_screenshot_color_mode(config)?;
                changed = true;
            }
            "--print-config" => print_config = true,
            other => bail!("Unknown argument: {other}. Use --help for supported options."),
        }
    }

    if changed {
        config.sync_linked_monitors();
        config.save()?;
        log::info!(
            "Saved updated Flux configuration to {:?}",
            AppConfig::config_path()
        );
    }

    if print_config {
        println!("{}", serde_json::to_string_pretty(config)?);
        return Ok(StartupAction::Exit);
    }

    Ok(StartupAction::Run { mode })
}

fn next_value<I>(args: &mut I, flag: &str) -> Result<String>
where
    I: Iterator<Item = String>,
{
    args.next()
        .with_context(|| format!("Expected a value after {flag}"))
}

pub fn apply_named_preset(config: &mut AppConfig, value: &str) -> Result<()> {
    let preset = match value.trim().to_ascii_lowercase().as_str() {
        "original" | "flux-original" | "flux_original" => ColorPreset::Original,
        "plasma" | "flux-plasma" | "flux_plasma" => ColorPreset::Plasma,
        "poolside" | "flux-poolside" | "flux_poolside" => ColorPreset::Poolside,
        "freedom" | "flux-freedom" | "flux_freedom" => ColorPreset::Freedom,
        _ => bail!("Unknown Flux preset '{value}'"),
    };
    config.apply_preset_to_active(preset);
    Ok(())
}

pub fn apply_image_color_mode(config: &mut AppConfig, path: &Path) -> Result<()> {
    let normalized = normalize_image_path(path)?;
    config.active_profile_mut().color_mode = ColorMode::ImageFile(normalized);
    Ok(())
}

fn normalize_image_path(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return Ok(path.to_path_buf());
    }
    bail!("Image not found at {}", path.display())
}

#[cfg(target_os = "macos")]
pub fn apply_current_wallpaper_color_mode(config: &mut AppConfig) -> Result<()> {
    use objc2_app_kit::{NSScreen, NSWorkspace};
    use objc2_foundation::MainThreadMarker;

    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let screen = NSScreen::mainScreen(mtm).context("Could not get the main screen")?;
    let workspace = NSWorkspace::sharedWorkspace();
    let wallpaper_url = workspace
        .desktopImageURLForScreen(&screen)
        .context("Could not get the current desktop image URL")?;
    let path = wallpaper_url
        .path()
        .map(|s| s.to_string())
        .context("Desktop image URL did not resolve to a file path")?;
    apply_image_color_mode(config, Path::new(&path))
}

#[cfg(not(target_os = "macos"))]
pub fn apply_current_wallpaper_color_mode(_config: &mut AppConfig) -> Result<()> {
    bail!("--wallpaper-image is currently implemented for macOS only")
}

#[cfg(target_os = "macos")]
pub fn apply_wallpaper_screenshot_color_mode(config: &mut AppConfig) -> Result<()> {
    let screenshot_path = capture_main_screen_to_temp_file()?;
    apply_image_color_mode(config, &screenshot_path)
}

#[cfg(not(target_os = "macos"))]
pub fn apply_wallpaper_screenshot_color_mode(_config: &mut AppConfig) -> Result<()> {
    bail!("--wallpaper-screenshot is currently implemented for macOS only")
}

#[cfg(target_os = "macos")]
fn capture_main_screen_to_temp_file() -> Result<PathBuf> {
    let path = std::env::temp_dir().join("drift-wallpaper-screenshot.png");
    let status = std::process::Command::new("screencapture")
        .arg("-x")
        .arg(&path)
        .status()
        .context("Failed to run screencapture")?;
    if !status.success() {
        return Err(anyhow!("screencapture exited with status {status}"));
    }
    Ok(path)
}

fn print_help() {
    println!(
        "\
drift-wallpaper

With no flags, runs Flux as a live wallpaper on your desktop.
Use --settings to open only the control panel.

Quick flags:
  --settings, --ui             Control panel window
  --background                 Same as default (explicit)
  --preview                    Large movable preview window instead of wallpaper windows

Flux profile:
  --preset <name>              One of: original, plasma, poolside, freedom
  --image <path>               Use an image file as the Flux color source
  --screenshot <path>          Same as --image
  --wallpaper-image            Use the current macOS wallpaper image as the color source
  --wallpaper-screenshot       Capture the current screen and use it as the color source
  --print-config               Print the current saved config and exit
  --help                       Show this help
"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_presets() {
        let mut config = AppConfig::default();
        apply_named_preset(&mut config, "plasma").unwrap();
        assert_eq!(
            config.active_profile().color_mode,
            ColorMode::Preset(ColorPreset::Plasma)
        );
    }
}
