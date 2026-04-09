use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use drift_core::{ColorPalette, NowPlayingSource};

#[derive(Debug, Clone)]
pub struct NowPlayingSnapshot {
    pub key: String,
    pub image_path: PathBuf,
    pub palette: [[f32; 3]; 3],
    pub accent_hex: String,
}

pub fn resolve_now_playing_snapshot(
    source: NowPlayingSource,
    previous_key: Option<&str>,
) -> Result<Option<NowPlayingSnapshot>> {
    #[cfg(target_os = "macos")]
    {
        match source {
            NowPlayingSource::Automatic => {
                if let Some(snapshot) = query_spotify(previous_key)? {
                    return Ok(Some(snapshot));
                }
                query_apple_music(previous_key)
            }
            NowPlayingSource::Spotify => query_spotify(previous_key),
            NowPlayingSource::AppleMusic => query_apple_music(previous_key),
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (source, previous_key);
        Ok(None)
    }
}

#[cfg(target_os = "macos")]
fn query_spotify(previous_key: Option<&str>) -> Result<Option<NowPlayingSnapshot>> {
    // Do not require `player state is playing` — Spotify often reports track changes while
    // briefly "paused" during crossfade; the boring.notch-style notification still fires then.
    let script = r#"
tell application "Spotify"
    try
        if player state is stopped then return ""
        set currentTrackName to name of current track
        set currentTrackArtist to artist of current track
        set currentTrackAlbum to album of current track
        try
            set artworkURL to artwork url of current track
        on error
            set artworkURL to ""
        end try
        return currentTrackName & linefeed & currentTrackArtist & linefeed & currentTrackAlbum & linefeed & artworkURL
    on error
        return ""
    end try
end tell
"#;

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .context("run Spotify now playing query")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::warn!(
            "Spotify osascript exited with {}: {} (grant Automation for Spotify to this app if empty track data)",
            output.status,
            stderr.trim()
        );
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut parts = stdout.lines();
    let title = parts.next().unwrap_or("").trim();
    let artist = parts.next().unwrap_or("").trim();
    let album = parts.next().unwrap_or("").trim();
    let artwork_url = parts.next().unwrap_or("").trim();
    if title.is_empty() || artist.is_empty() || album.is_empty() {
        return Ok(None);
    }

    let key = format!("spotify::{title}::{artist}::{album}::{artwork_url}");
    let image_path = artwork_cache_path(
        "spotify",
        &key,
        if artwork_url.is_empty() { "png" } else { "jpg" },
    );
    if previous_key == Some(key.as_str()) && image_path.exists() {
        let palette = palette_from_image(&image_path);
        return Ok(Some(NowPlayingSnapshot {
            accent_hex: rgb_to_hex(palette[1]),
            palette,
            key,
            image_path,
        }));
    }

    if artwork_url.is_empty() {
        let palette = palette_from_track_identity(title, artist, album);
        write_solid_palette_png(&image_path, &palette)?;
        return Ok(Some(NowPlayingSnapshot {
            accent_hex: rgb_to_hex(palette[1]),
            palette,
            key,
            image_path,
        }));
    }

    let status = Command::new("curl")
        .arg("-L")
        .arg("-f")
        .arg("-sS")
        .arg(artwork_url)
        .arg("-o")
        .arg(&image_path)
        .status()
        .context("download Spotify artwork")?;
    if !status.success() {
        let palette = palette_from_track_identity(title, artist, album);
        write_solid_palette_png(&image_path, &palette)?;
        return Ok(Some(NowPlayingSnapshot {
            accent_hex: rgb_to_hex(palette[1]),
            palette,
            key,
            image_path,
        }));
    }

    let palette = palette_from_image(&image_path);
    Ok(Some(NowPlayingSnapshot {
        accent_hex: rgb_to_hex(palette[1]),
        palette,
        key,
        image_path,
    }))
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
fn write_solid_palette_png(path: &Path, palette: &[[f32; 3]; 3]) -> Result<()> {
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

#[cfg(target_os = "macos")]
fn query_apple_music(previous_key: Option<&str>) -> Result<Option<NowPlayingSnapshot>> {
    let script = r#"
tell application "Music"
    if player state is not playing then error "Music is not playing"
    set currentTrackName to name of current track
    set currentTrackArtist to artist of current track
    set currentTrackAlbum to album of current track
    try
        set artworkData to data of artwork 1 of current track
    on error
        error "Music track has no artwork"
    end try
end tell

set tempPath to do shell script "mktemp /tmp/drift-now-playing-music-XXXXXX"
set fileRef to open for access (POSIX file tempPath) with write permission
set eof of fileRef to 0
write artworkData to fileRef
close access fileRef
return currentTrackName & linefeed & currentTrackArtist & linefeed & currentTrackAlbum & linefeed & tempPath
"#;

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .context("run Apple Music now playing query")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::warn!(
            "Apple Music osascript exited with {}: {}",
            output.status,
            stderr.trim()
        );
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut parts = stdout.lines();
    let title = parts.next().unwrap_or("").trim();
    let artist = parts.next().unwrap_or("").trim();
    let album = parts.next().unwrap_or("").trim();
    let temp_path = parts.next().unwrap_or("").trim();
    if title.is_empty() || artist.is_empty() || album.is_empty() || temp_path.is_empty() {
        return Ok(None);
    }

    let key = format!("apple_music::{title}::{artist}::{album}");
    let image_path = artwork_cache_path("apple-music", &key, "png");
    if previous_key == Some(key.as_str()) && image_path.exists() {
        let _ = std::fs::remove_file(temp_path);
        let palette = palette_from_image(&image_path);
        return Ok(Some(NowPlayingSnapshot {
            accent_hex: rgb_to_hex(palette[1]),
            palette,
            key,
            image_path,
        }));
    }

    std::fs::rename(temp_path, &image_path)
        .or_else(|_| std::fs::copy(temp_path, &image_path).map(|_| ()))
        .with_context(|| format!("cache Apple Music artwork to {}", image_path.display()))?;

    let palette = palette_from_image(&image_path);
    Ok(Some(NowPlayingSnapshot {
        accent_hex: rgb_to_hex(palette[1]),
        palette,
        key,
        image_path,
    }))
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
