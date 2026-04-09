use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use drift_core::color::{ColorPalette, Preset};

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
    // Default: live wallpaper on the desktop (see --settings for control panel only).
    let mut mode = RunMode::Background;
    let mut changed = false;
    let mut print_config = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                print_help();
                return Ok(StartupAction::Exit);
            }
            "--settings" | "--ui" => {
                mode = RunMode::Ui;
            }
            "--background" => {
                mode = RunMode::Background;
            }
            "--preview" => {
                mode = RunMode::Preview;
            }
            "--preset" => {
                let value = next_value(&mut args, "--preset")?;
                apply_named_preset(config, &value)?;
                changed = true;
            }
            "--colors" => {
                let value = next_value(&mut args, "--colors")?;
                apply_custom_colors(config, &value)?;
                changed = true;
            }
            "--image" | "--screenshot" => {
                let value = next_value(&mut args, arg.as_str())?;
                apply_image_palette(config, Path::new(&value))?;
                changed = true;
            }
            "--wallpaper-image" => {
                apply_current_wallpaper_palette(config)?;
                changed = true;
            }
            "--wallpaper-screenshot" => {
                apply_wallpaper_screenshot_palette(config)?;
                changed = true;
            }
            "--print-config" => {
                print_config = true;
            }
            other => {
                bail!("Unknown argument: {other}. Use --help for supported options.");
            }
        }
    }

    if changed {
        config.save()?;
        log::info!(
            "Saved updated color configuration to {:?}",
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
    let preset = parse_preset(value)?;
    apply_palette(config, &ColorPalette::preset(preset));
    Ok(())
}

pub fn apply_custom_colors(config: &mut AppConfig, value: &str) -> Result<()> {
    let [a, b, c] = parse_color_triplet(value)?;
    apply_palette(config, &ColorPalette::from_stops(a, b, c));
    Ok(())
}

fn parse_preset(value: &str) -> Result<Preset> {
    match value.trim().to_ascii_lowercase().as_str() {
        "flux_original" | "flux-original" | "original" | "drift" => Ok(Preset::FluxOriginal),
        "flux_plasma" | "flux-plasma" | "plasma" => Ok(Preset::FluxPlasma),
        "flux_poolside" | "flux-poolside" | "poolside" => Ok(Preset::FluxPoolside),
        "flux_freedom" | "flux-freedom" | "freedom" => Ok(Preset::FluxFreedom),
        "ocean" => Ok(Preset::Ocean),
        "sunset" => Ok(Preset::Sunset),
        "forest" => Ok(Preset::Forest),
        "lava" => Ok(Preset::Lava),
        "midnight" => Ok(Preset::Midnight),
        "monochrome" | "mono" => Ok(Preset::Monochrome),
        _ => bail!("Unknown preset '{value}'"),
    }
}

pub fn parse_color_triplet(value: &str) -> Result<[[f32; 3]; 3]> {
    let parts = value.split(',').map(str::trim).collect::<Vec<_>>();
    if parts.len() != 3 {
        bail!("--colors expects exactly three comma-separated colors");
    }
    Ok([
        parse_hex_color(parts[0])?,
        parse_hex_color(parts[1])?,
        parse_hex_color(parts[2])?,
    ])
}

fn parse_hex_color(value: &str) -> Result<[f32; 3]> {
    let trimmed = value.trim().trim_start_matches('#');
    let expanded = match trimmed.len() {
        3 => {
            let chars = trimmed.chars().collect::<Vec<_>>();
            format!(
                "{}{}{}{}{}{}",
                chars[0], chars[0], chars[1], chars[1], chars[2], chars[2]
            )
        }
        6 => trimmed.to_owned(),
        _ => bail!("Invalid color '{value}'. Expected #RGB or #RRGGBB."),
    };

    let r = u8::from_str_radix(&expanded[0..2], 16)?;
    let g = u8::from_str_radix(&expanded[2..4], 16)?;
    let b = u8::from_str_radix(&expanded[4..6], 16)?;
    Ok([(r as f32) / 255.0, (g as f32) / 255.0, (b as f32) / 255.0])
}

pub fn apply_image_palette(config: &mut AppConfig, path: &Path) -> Result<()> {
    let image = load_image(path)?;
    let palette = ColorPalette::from_image(&image);
    apply_palette(config, &palette);
    Ok(())
}

fn apply_palette(config: &mut AppConfig, palette: &ColorPalette) {
    let speed = config.params.speed;
    let scale = config.params.scale;
    let target_fps = config.params.target_fps;
    config.params = drift_core::DriftParams::from_palette(palette, speed);
    config.params.scale = scale;
    config.params.target_fps = target_fps;
}

fn load_image(path: &Path) -> Result<image::DynamicImage> {
    match image::open(path) {
        Ok(image) => Ok(image),
        Err(error) => load_image_with_platform_fallback(path, error),
    }
}

#[cfg(target_os = "macos")]
fn load_image_with_platform_fallback(
    path: &Path,
    original_error: image::ImageError,
) -> Result<image::DynamicImage> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase());

    if matches!(extension.as_deref(), Some("heic" | "heif")) {
        let converted_path = std::env::temp_dir().join("drift-wallpaper-converted.png");
        let status = std::process::Command::new("sips")
            .args(["-s", "format", "png"])
            .arg(path)
            .args(["--out"])
            .arg(&converted_path)
            .status()
            .context("Failed to run sips for HEIC conversion")?;

        if status.success() {
            return image::open(&converted_path)
                .with_context(|| format!("Open converted image at {}", converted_path.display()));
        }
    }

    Err(original_error).with_context(|| format!("Open image at {}", path.display()))
}

#[cfg(not(target_os = "macos"))]
fn load_image_with_platform_fallback(
    path: &Path,
    original_error: image::ImageError,
) -> Result<image::DynamicImage> {
    Err(original_error).with_context(|| format!("Open image at {}", path.display()))
}

#[cfg(target_os = "macos")]
pub fn apply_current_wallpaper_palette(config: &mut AppConfig) -> Result<()> {
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
    apply_image_palette(config, Path::new(&path))
}

#[cfg(not(target_os = "macos"))]
pub fn apply_current_wallpaper_palette(_config: &mut AppConfig) -> Result<()> {
    bail!("--wallpaper-image is currently implemented for macOS only")
}

#[cfg(target_os = "macos")]
pub fn apply_wallpaper_screenshot_palette(config: &mut AppConfig) -> Result<()> {
    let screenshot_path = capture_main_screen_to_temp_file()?;
    apply_image_palette(config, &screenshot_path)
}

#[cfg(not(target_os = "macos"))]
pub fn apply_wallpaper_screenshot_palette(_config: &mut AppConfig) -> Result<()> {
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

With no flags, runs the live wallpaper on your desktop (respects saved on/off in config).
Use --settings to open only the control panel.

Quick flags:
  --settings, --ui             Control panel window (tray icon also on macOS unless spawned from tray)
  --background                 Same as default (explicit)
  --preview                    Large movable preview window instead of full-desktop wallpaper

Palette / config:
  --preset <name>              Use a built-in palette
  --colors <c1,c2,c3>          Set three custom colors, e.g. '#0a1020,#4060b0,#f5d070'
  --image <path>               Extract colors from an image file
  --screenshot <path>          Extract colors from a screenshot file
  --wallpaper-image            Extract colors from the current macOS wallpaper image
  --wallpaper-screenshot       Capture the current screen and extract colors from it
  --print-config               Print the current saved config and exit
  --help                       Show this help
"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_colors() {
        assert_eq!(
            parse_hex_color("#ff8040").unwrap(),
            [1.0, 128.0 / 255.0, 64.0 / 255.0]
        );
        assert_eq!(
            parse_hex_color("#abc").unwrap(),
            [170.0 / 255.0, 187.0 / 255.0, 204.0 / 255.0]
        );
    }

    #[test]
    fn parses_color_triplets() {
        let triplet = parse_color_triplet("#000000,#ffffff,#123456").unwrap();
        assert_eq!(triplet[0], [0.0, 0.0, 0.0]);
        assert_eq!(triplet[1], [1.0, 1.0, 1.0]);
        assert_eq!(triplet[2], [18.0 / 255.0, 52.0 / 255.0, 86.0 / 255.0]);
    }
}
