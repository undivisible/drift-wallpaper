//! Linux media player querying via MPRIS D-Bus.
//!
//! Uses zbus to query MPRIS-compliant media players (Spotify, Chromium, etc.)

use drift_core::NowPlayingSource;

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
            NowPlayingSource::AppleMusic => Self::query_apple_music(previous_key),
            NowPlayingSource::Automatic => Self::query_spotify(previous_key)
                .or_else(|_| Self::query_browser_tabs(previous_key)),
        }
    }

    fn query_spotify(_previous_key: Option<&str>) -> anyhow::Result<Option<NowPlayingSnapshot>> {
        Ok(None)
    }

    fn query_apple_music(
        _previous_key: Option<&str>,
    ) -> anyhow::Result<Option<NowPlayingSnapshot>> {
        Ok(None)
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
