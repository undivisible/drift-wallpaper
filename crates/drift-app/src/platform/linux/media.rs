//! Linux media player querying via MPRIS D-Bus.
//!
//! Uses zbus to query MPRIS-compliant media players (Spotify, Chromium, etc.)

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use drift_core::{ColorPalette, NowPlayingSource};
use zbus::Connection;

use crate::media_art::NowPlayingSnapshot;
use crate::platform::MediaQuerier;

pub struct LinuxMediaQuerier;

impl LinuxMediaQuerier {
    pub fn query_now_playing(
        source: NowPlayingSource,
        previous_key: Option<&str>,
    ) -> anyhow::Result<Option<NowPlayingSnapshot>> {
        match source {
            NowPlayingSource::Spotify => Self::query_spotify(previous_key),
            NowPlayingSource::AppleMusic => Ok(None),
            NowPlayingSource::Automatic => Self::query_spotify(previous_key)
                .or_else(|_| Self::query_browser_tabs(previous_key)),
        }
    }

    fn query_spotify(previous_key: Option<&str>) -> anyhow::Result<Option<NowPlayingSnapshot>> {
        let proxy = zbus::Proxy::new(
            &Connection::session()?,
            "org.mpris.MediaPlayer2.spotify",
            "/org/mpris/MediaPlayer2",
            "org.mpris.MediaPlayer2",
        )
        .ok();

        let proxy = match proxy {
            Some(p) => p,
            None => return Ok(None),
        };

        let playback_status: String = proxy
            .get_property("PlaybackStatus")
            .context("get PlaybackStatus")?;

        if playback_status != "Playing" {
            return Ok(None);
        }

        let metadata: zbus::zvariant::Dict<String, zbus::zvariant::OwnedValue, _> =
            proxy.get_property("Metadata").context("get Metadata")?;

        let track_id = metadata
            .get("xesam:url")
            .and_then(|v| v.downcast_ref::<String>())
            .map(|s| s.as_str())
            .unwrap_or("");

        let title = metadata
            .get("xesam:title")
            .and_then(|v| v.downcast_ref::<String>())
            .map(|s| s.as_str())
            .unwrap_or("");

        let artist = metadata
            .get("xesam:artist")
            .and_then(|v| v.downcast_ref::<Vec<String>>())
            .map(|v| v.join(", "))
            .unwrap_or_default();

        let album = metadata
            .get("xesam:album")
            .and_then(|v| v.downcast_ref::<String>())
            .map(|s| s.as_str())
            .unwrap_or("");

        let art_url = metadata
            .get("mpris:artUrl")
            .and_then(|v| v.downcast_ref::<String>())
            .map(|s| s.as_str())
            .unwrap_or("");

        if title.is_empty() || artist.is_empty() {
            return Ok(None);
        }

        let key = if !track_id.is_empty() {
            format!("spotify::{}", track_id)
        } else {
            format!(
                "spotify::fallback::{}::{}::{}::{}",
                title, artist, album, art_url
            )
        };

        let image_path = artwork_cache_path("spotify", &key, "jpg");
        if previous_key == Some(key.as_str()) && image_path.exists() {
            let palette = palette_from_image(&image_path);
            return Ok(Some(NowPlayingSnapshot {
                key,
                image_path,
                palette,
                accent_hex: rgb_to_hex(palette[1]),
            }));
        }

        if art_url.is_empty() {
            let palette = palette_from_track_identity(title, artist, album);
            write_solid_palette_png(&image_path, &palette)?;
            return Ok(Some(NowPlayingSnapshot {
                key,
                image_path,
                palette,
                accent_hex: rgb_to_hex(palette[1]),
            }));
        }

        let status = Command::new("curl")
            .arg("-L")
            .arg("-f")
            .arg("-sS")
            .arg("-A")
            .arg("DriftWallpaper/1.0")
            .arg(art_url)
            .arg("-o")
            .arg(&image_path)
            .status()
            .context("download Spotify artwork")?;

        if !status.success() {
            let palette = palette_from_track_identity(title, artist, album);
            write_solid_palette_png(&image_path, &palette)?;
            return Ok(Some(NowPlayingSnapshot {
                key,
                image_path,
                palette,
                accent_hex: rgb_to_hex(palette[1]),
            }));
        }

        let palette = palette_from_image(&image_path);
        Ok(Some(NowPlayingSnapshot {
            key,
            image_path,
            palette,
            accent_hex: rgb_to_hex(palette[1]),
        }))
    }

    fn query_browser_tabs(
        _previous_key: Option<&str>,
    ) -> anyhow::Result<Option<NowPlayingSnapshot>> {
        Ok(None)
    }
}

impl MediaQuerier for LinuxMediaQuerier {
    fn query_now_playing(
        source: NowPlayingSource,
        previous_key: Option<&str>,
    ) -> anyhow::Result<Option<NowPlayingSnapshot>> {
        Self::query_now_playing(source, previous_key)
    }
}

fn artwork_cache_path(prefix: &str, key: &str, extension: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    let hash = hasher.finish();
    std::env::temp_dir().join(format!(
        "drift-now-playing-{prefix}-{hash:016x}.{extension}"
    ))
}

fn palette_from_image(path: &PathBuf) -> [[f32; 3]; 3] {
    image::open(path)
        .ok()
        .map(|img| ColorPalette::from_image(&img).colors())
        .unwrap_or_else(fallback_palette)
}

fn rgb_to_hex(rgb: [f32; 3]) -> String {
    let r = (rgb[0].clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (rgb[1].clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = (rgb[2].clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn fallback_palette() -> [[f32; 3]; 3] {
    [[0.02, 0.04, 0.18], [0.12, 0.38, 0.72], [0.85, 0.94, 0.98]]
}

fn palette_from_track_identity(title: &str, artist: &str, album: &str) -> [[f32; 3]; 3] {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    title.hash(&mut hasher);
    artist.hash(&mut hasher);
    album.hash(&mut hasher);
    let h = hasher.finish();
    let f = |bits: u64, shift: u32| (((bits >> shift) & 0xff) as f32) / 255.0;
    let a = [f(h, 0), f(h, 8), f(h, 16)];
    let b = [f(h, 24), f(h, 32), f(h, 40).max(0.15)];
    let c = [f(h, 48), f(h, 56), (a[0] * 0.4 + b[1] * 0.6).min(1.0)];
    [a, b, c]
}

fn write_solid_palette_png(path: &PathBuf, palette: &[[f32; 3]; 3]) -> Result<()> {
    let mut img = image::RgbaImage::new(256, 128);
    let mid = palette[1];
    let px = image::Rgba([
        (mid[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (mid[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (mid[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        255,
    ]);
    for y in 0..128 {
        for x in 0..256 {
            img.put_pixel(x, y, px);
        }
    }
    img.save(path)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}
