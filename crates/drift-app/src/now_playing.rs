use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use drift_core::NowPlayingSource;

use crate::{
    config::AppConfig,
    media_art::{self, NowPlayingSnapshot},
};

#[cfg(target_os = "macos")]
use objc2::{define_class, extern_methods, sel};

#[cfg(target_os = "macos")]
use objc2::rc::{autoreleasepool, Retained};

#[cfg(target_os = "macos")]
use objc2::runtime::AnyObject;

#[cfg(target_os = "macos")]
use objc2_foundation::{
    NSDate, NSDistributedNotificationCenter, NSNotificationSuspensionBehavior, NSObject,
    NSObjectProtocol, NSRunLoop, NSString,
};

static REFRESH_TX: OnceLock<mpsc::Sender<()>> = OnceLock::new();

const TRANSITION_DURATION: Duration = Duration::from_millis(600);
const IDLE_TICK: Duration = Duration::from_millis(250);
const TRANSITION_TICK: Duration = Duration::from_millis(50);
const TRANSITION_STEPS: u32 = 12;

/// After a Spotify playback notification fires, poll every 500 ms for up to this long before
/// falling back to the normal refresh interval. Spotify's internal state sometimes lags behind
/// the notification by a few hundred milliseconds.
const SPOTIFY_NOTIFICATION_RETRY_WINDOW: Duration = Duration::from_secs(5);
const SPOTIFY_NOTIFICATION_RETRY_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone)]
pub struct NowPlayingUpdate {
    pub snapshot: Option<NowPlayingSnapshot>,
}

pub struct NowPlayingController {
    refresh_tx: mpsc::Sender<()>,
}

impl NowPlayingController {
    pub fn request_refresh(&self) {
        let _ = self.refresh_tx.send(());
    }
}

pub fn spawn_now_playing_worker(
    config: Arc<Mutex<AppConfig>>,
) -> (NowPlayingController, mpsc::Receiver<NowPlayingUpdate>) {
    let (refresh_tx, refresh_rx) = mpsc::channel();
    let (update_tx, update_rx) = mpsc::channel();
    let controller = NowPlayingController {
        refresh_tx: refresh_tx.clone(),
    };

    let _ = REFRESH_TX.set(refresh_tx.clone());

    thread::spawn(move || {
        worker_loop(config, refresh_rx, update_tx);
    });

    (controller, update_rx)
}

#[cfg(target_os = "macos")]
define_class!(
    #[unsafe(super(NSObject))]
    struct SpotifyPlaybackObserver;

    unsafe impl NSObjectProtocol for SpotifyPlaybackObserver {}

    impl SpotifyPlaybackObserver {
        #[unsafe(method(spotifyPlaybackChanged:))]
        fn spotify_playback_changed(&self, _sender: Option<&AnyObject>) {
            if let Some(tx) = REFRESH_TX.get() {
                let _ = tx.send(());
            }
        }
    }
);

#[cfg(target_os = "macos")]
impl SpotifyPlaybackObserver {
    extern_methods!(
        #[unsafe(method(new))]
        fn new() -> Retained<Self>;
    );

    fn new_observer() -> Retained<Self> {
        Self::new()
    }
}

#[derive(Clone)]
struct TransitionState {
    from: NowPlayingSnapshot,
    to: NowPlayingSnapshot,
    source: Option<NowPlayingSource>,
    started_at: Instant,
    last_step: u32,
}

impl TransitionState {
    fn new(
        from: NowPlayingSnapshot,
        to: NowPlayingSnapshot,
        source: Option<NowPlayingSource>,
    ) -> Self {
        Self {
            from,
            to,
            source,
            started_at: Instant::now(),
            last_step: u32::MAX,
        }
    }
}

fn worker_loop(
    config: Arc<Mutex<AppConfig>>,
    refresh_rx: mpsc::Receiver<()>,
    update_tx: mpsc::Sender<NowPlayingUpdate>,
) {
    #[cfg(target_os = "macos")]
    let _observer = start_spotify_observer();

    let mut last_source: Option<NowPlayingSource> = None;
    let mut settled_snapshot: Option<NowPlayingSnapshot> = None;
    let mut displayed_snapshot: Option<NowPlayingSnapshot> = None;
    let mut transition: Option<TransitionState> = None;
    let mut last_refresh = Instant::now() - Duration::from_secs(60);
    let mut pending_refresh = true;
    // Track when we last received a Spotify playback notification so we can retry quickly
    // if Spotify's state hasn't caught up to the notification yet.
    let mut last_notification_at: Option<Instant> = None;

    loop {
        let got_notification = refresh_rx.try_recv().is_ok();
        // Drain any remaining signals.
        while refresh_rx.try_recv().is_ok() {}
        if got_notification {
            pending_refresh = true;
            last_notification_at = Some(Instant::now());
        }

        let current_source = config
            .lock()
            .ok()
            .and_then(|cfg| cfg.wallpaper_profile().color_mode.now_playing_source());

        if current_source != last_source {
            pending_refresh = true;
        }

        // If we recently received a Spotify notification and haven't detected a change yet,
        // poll frequently to catch up with Spotify's lagging internal state.
        let in_spotify_retry_window =
            last_notification_at.is_some_and(|t| t.elapsed() < SPOTIFY_NOTIFICATION_RETRY_WINDOW);

        let refresh_interval = if transition.is_some() {
            TRANSITION_TICK
        } else if in_spotify_retry_window {
            SPOTIFY_NOTIFICATION_RETRY_INTERVAL
        } else {
            match current_source {
                // Faster polling when a desktop player is selected — distributed
                // notifications are best-effort; AppleScript rounds out track changes.
                Some(NowPlayingSource::Spotify) => Duration::from_millis(900),
                Some(NowPlayingSource::AppleMusic) => Duration::from_secs(3),
                Some(NowPlayingSource::Automatic) => Duration::from_secs(6),
                None => Duration::from_secs(30),
            }
        };

        if pending_refresh || last_refresh.elapsed() >= refresh_interval {
            match resolve_now_playing(&config, current_source, settled_snapshot.as_ref()) {
                Ok(Some(snapshot)) => {
                    let changed = settled_snapshot
                        .as_ref()
                        .is_none_or(|current| current.key != snapshot.key);

                    if changed {
                        // Clear the retry window once we've detected the new track.
                        last_notification_at = None;
                        if let Some(from) = displayed_snapshot.clone() {
                            transition = Some(TransitionState::new(from, snapshot, current_source));
                        } else {
                            settle_snapshot(
                                &config,
                                &update_tx,
                                &mut last_source,
                                &mut settled_snapshot,
                                &mut displayed_snapshot,
                                current_source,
                                snapshot,
                            );
                        }
                    }
                }
                Ok(None) => {
                    transition = None;
                    settled_snapshot = None;
                    displayed_snapshot = None;
                    last_source = current_source;
                    last_notification_at = None;
                    apply_ui_accent(&config, None);
                    let _ = update_tx.send(NowPlayingUpdate { snapshot: None });
                }
                Err(error) => {
                    log::warn!("refresh now playing artwork: {error}");
                }
            }

            last_refresh = Instant::now();
            pending_refresh = false;
        }

        if let Some(mut active_transition) = transition.take() {
            let progress = active_transition.started_at.elapsed().as_secs_f32()
                / TRANSITION_DURATION.as_secs_f32();
            let progress = progress.clamp(0.0, 1.0);
            let step = ((progress * TRANSITION_STEPS as f32).round() as u32).min(TRANSITION_STEPS);

            if step > active_transition.last_step || progress >= 1.0 {
                let blended_palette = blend_palettes(
                    active_transition.from.palette,
                    active_transition.to.palette,
                    smoothstep(progress),
                );
                let snapshot = if progress >= 1.0 {
                    active_transition.to.clone()
                } else {
                    match write_transition_snapshot(
                        &active_transition.from,
                        &active_transition.to,
                        step,
                        blended_palette,
                    ) {
                        Ok(snapshot) => snapshot,
                        Err(error) => {
                            log::warn!("render now playing transition frame: {error}");
                            active_transition.to.clone()
                        }
                    }
                };

                active_transition.last_step = step;
                displayed_snapshot = Some(snapshot.clone());
                apply_ui_accent(&config, Some(snapshot.accent_hex.clone()));
                let _ = update_tx.send(NowPlayingUpdate {
                    snapshot: Some(snapshot.clone()),
                });

                if progress >= 1.0 {
                    settled_snapshot = Some(active_transition.to.clone());
                    displayed_snapshot = settled_snapshot.clone();
                    last_source = active_transition.source;
                    transition = None;
                } else {
                    transition = Some(active_transition);
                }
            } else {
                transition = Some(active_transition);
            }
        }

        #[cfg(target_os = "macos")]
        {
            autoreleasepool(|_| {
                let run_loop = NSRunLoop::currentRunLoop();
                let delay = if transition.is_some() {
                    TRANSITION_TICK
                } else {
                    IDLE_TICK
                };
                let deadline = NSDate::dateWithTimeIntervalSinceNow(delay.as_secs_f64());
                run_loop.runUntilDate(&deadline);
            });
        }

        #[cfg(not(target_os = "macos"))]
        {
            let delay = if transition.is_some() {
                TRANSITION_TICK
            } else {
                IDLE_TICK
            };
            thread::sleep(delay);
        }
    }
}

fn settle_snapshot(
    config: &Arc<Mutex<AppConfig>>,
    update_tx: &mpsc::Sender<NowPlayingUpdate>,
    last_source: &mut Option<NowPlayingSource>,
    settled_snapshot: &mut Option<NowPlayingSnapshot>,
    displayed_snapshot: &mut Option<NowPlayingSnapshot>,
    source: Option<NowPlayingSource>,
    snapshot: NowPlayingSnapshot,
) {
    apply_ui_accent(config, Some(snapshot.accent_hex.clone()));
    *last_source = source;
    *settled_snapshot = Some(snapshot.clone());
    *displayed_snapshot = Some(snapshot.clone());
    let _ = update_tx.send(NowPlayingUpdate {
        snapshot: Some(snapshot),
    });
}

fn resolve_now_playing(
    config: &Arc<Mutex<AppConfig>>,
    current_source: Option<NowPlayingSource>,
    previous_snapshot: Option<&NowPlayingSnapshot>,
) -> anyhow::Result<Option<NowPlayingSnapshot>> {
    match current_source {
        Some(source) => media_art::resolve_now_playing_snapshot(
            source,
            previous_snapshot.map(|snapshot| snapshot.key.as_str()),
        ),
        None => {
            apply_ui_accent(config, None);
            Ok(None)
        }
    }
}

fn apply_ui_accent(config: &Arc<Mutex<AppConfig>>, accent: Option<String>) {
    let mut to_save = None;
    if let Ok(mut cfg) = config.lock() {
        if cfg.now_playing_accent_hex != accent {
            cfg.now_playing_accent_hex = accent;
            to_save = Some(cfg.clone());
        }
    }

    if let Some(cfg) = to_save {
        if let Err(error) = cfg.save() {
            log::warn!("save now playing accent: {error}");
        }
    }
}

fn write_transition_snapshot(
    from: &NowPlayingSnapshot,
    to: &NowPlayingSnapshot,
    step: u32,
    palette: [[f32; 3]; 3],
) -> anyhow::Result<NowPlayingSnapshot> {
    let path = transition_cache_path(&from.key, &to.key, step);
    let image = render_palette_image(palette, 256, 128);
    image.save(&path)?;

    Ok(NowPlayingSnapshot {
        key: format!("transition::{}::{}::{step}", from.key, to.key),
        image_path: path,
        palette,
        accent_hex: rgb_to_hex(palette[1]),
    })
}

fn transition_cache_path(from_key: &str, to_key: &str, step: u32) -> PathBuf {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    from_key.hash(&mut hasher);
    to_key.hash(&mut hasher);
    let hash = hasher.finish();
    std::env::temp_dir().join(format!(
        "drift-now-playing-transition-{hash:016x}-{step:02}.png"
    ))
}

fn render_palette_image(palette: [[f32; 3]; 3], width: u32, height: u32) -> image::RgbaImage {
    let mut image = image::RgbaImage::new(width, height);
    if width == 0 || height == 0 {
        return image;
    }

    for y in 0..height {
        let _ = y;
        for x in 0..width {
            let t = if width <= 1 {
                0.0
            } else {
                x as f32 / (width - 1) as f32
            };
            let rgb = sample_three_stop_palette(palette, t);
            image.put_pixel(
                x,
                y,
                image::Rgba([
                    (rgb[0].clamp(0.0, 1.0) * 255.0).round() as u8,
                    (rgb[1].clamp(0.0, 1.0) * 255.0).round() as u8,
                    (rgb[2].clamp(0.0, 1.0) * 255.0).round() as u8,
                    255,
                ]),
            );
        }
    }

    image
}

fn sample_three_stop_palette(stops: [[f32; 3]; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    if t <= 0.5 {
        lerp3(stops[0], stops[1], t * 2.0)
    } else {
        lerp3(stops[1], stops[2], (t - 0.5) * 2.0)
    }
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn blend_palettes(from: [[f32; 3]; 3], to: [[f32; 3]; 3], t: f32) -> [[f32; 3]; 3] {
    std::array::from_fn(|index| lerp3(from[index], to[index], t))
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(target_os = "macos")]
fn start_spotify_observer() -> Option<Retained<SpotifyPlaybackObserver>> {
    let observer = SpotifyPlaybackObserver::new_observer();
    let center = NSDistributedNotificationCenter::defaultCenter();
    let name = NSString::from_str("com.spotify.client.PlaybackStateChanged");

    unsafe {
        center.addObserver_selector_name_object_suspensionBehavior(
            &observer,
            sel!(spotifyPlaybackChanged:),
            Some(&name),
            None,
            NSNotificationSuspensionBehavior::DeliverImmediately,
        );
    }

    Some(observer)
}

fn rgb_to_hex(rgb: [f32; 3]) -> String {
    let r = (rgb[0].clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (rgb[1].clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = (rgb[2].clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{r:02x}{g:02x}{b:02x}")
}
