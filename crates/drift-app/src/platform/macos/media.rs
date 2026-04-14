//! macOS media player querying via osascript.

use drift_core::NowPlayingSource;

use crate::media_art::NowPlayingSnapshot;
use crate::platform::MediaQuerier;

pub struct MacosMediaQuerier;

impl MediaQuerier for MacosMediaQuerier {
    fn query_now_playing(
        source: NowPlayingSource,
        previous_key: Option<&str>,
    ) -> anyhow::Result<Option<NowPlayingSnapshot>> {
        crate::media_art::resolve_now_playing_snapshot(source, previous_key)
    }
}
