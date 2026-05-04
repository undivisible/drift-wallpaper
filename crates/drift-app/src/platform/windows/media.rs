//! Windows media player querying via Windows Media Session API.
//!
//! Uses the Windows Media Session API (via the windows crate) to query
//! now playing information from media players like Spotify.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use drift_core::{ColorPalette, NowPlayingSource};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};

use crate::media_art::NowPlayingSnapshot;
use crate::platform::MediaQuerier;

pub struct WindowsMediaQuerier;

impl WindowsMediaQuerier {
    pub fn query_now_playing(
        source: NowPlayingSource,
        previous_key: Option<&str>,
    ) -> anyhow::Result<Option<NowPlayingSnapshot>> {
        match source {
            NowPlayingSource::Spotify => Self::query_spotify(previous_key),
            NowPlayingSource::AppleMusic => Self::query_apple_music(previous_key),
            NowPlayingSource::Automatic => {
                Self::query_spotify(previous_key).or_else(|_| Self::query_apple_music(previous_key))
            }
        }
    }

    fn query_spotify(previous_key: Option<&str>) -> anyhow::Result<Option<NowPlayingSnapshot>> {
        Self::query_media_player("Spotify", previous_key)
    }

    fn query_apple_music(previous_key: Option<&str>) -> anyhow::Result<Option<NowPlayingSnapshot>> {
        Self::query_media_player("Apple Music", previous_key)
    }

    fn query_media_player(
        app_name: &str,
        previous_key: Option<&str>,
    ) -> anyhow::Result<Option<NowPlayingSnapshot>> {
        let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?
            .get()
            .context("get media session manager")?;

        let session = manager
            .GetSession(app_name)
            .context("get media session for app")?;

        let playback_info = session.GetPlaybackInfo().context("get playback info")?;

        let status = playback_info
            .PlaybackStatus()
            .context("get playback status")?;

        if status != GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing {
            return Ok(None);
        }

        let timeline = session
            .GetTimelineProperties()
            .context("get timeline properties")?;

        let media_props = session
            .TryGetMediaPropertiesAsync()?
            .get()
            .context("get media properties")?;

        let title = media_props.Title().context("get track title")?.to_string();

        let artist = media_props
            .Artist()
            .context("get track artist")?
            .to_string();

        let album = media_props
            .AlbumTitle()
            .context("get album title")?
            .to_string();

        if title.is_empty() || artist.is_empty() {
            return Ok(None);
        }

        let track_id = format!(
            "{}:{}:{}",
            app_name,
            timeline.StartTime()?,
            timeline.EndTime()?
        );

        let key = format!("windows::{}::{}::{}", app_name, title, artist);
        let image_path = artwork_cache_path(app_name.to_lowercase().as_str(), &key, "jpg");

        if previous_key == Some(key.as_str()) && image_path.exists() {
            let palette = palette_from_image(&image_path);
            return Ok(Some(NowPlayingSnapshot {
                key,
                image_path,
                palette,
                accent_hex: rgb_to_hex(palette[1]),
            }));
        }

        if let Ok(thumbnail) = media_props.Thumbnail() {
            if !thumbnail.is_empty() {
                let data = thumbnail.get()?;
                std::fs::write(&image_path, data)?;
                let palette = palette_from_image(&image_path);
                return Ok(Some(NowPlayingSnapshot {
                    key,
                    image_path,
                    palette,
                    accent_hex: rgb_to_hex(palette[1]),
                }));
            }
        }

        let palette = palette_from_track_identity(&title, &artist, &album);
        write_solid_palette_png(&image_path, &palette)?;
        Ok(Some(NowPlayingSnapshot {
            key,
            image_path,
            palette,
            accent_hex: rgb_to_hex(palette[1]),
        }))
    }
}

impl MediaQuerier for WindowsMediaQuerier {
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
