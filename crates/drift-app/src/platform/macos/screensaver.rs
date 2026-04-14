//! macOS screensaver support.
//!
//! macOS screensavers are bundled as .saver packages, which requires a different
//! implementation approach than Windows/Linux. For now, we provide a no-op
//! implementation.

use crate::platform::{ScreensaverMode, ScreensaverRunner};

pub struct MacosScreensaverRunner;

impl ScreensaverRunner for MacosScreensaverRunner {
    fn parse_screensaver_args() -> ScreensaverMode {
        ScreensaverMode::None
    }

    fn configure_as_screensaver() -> anyhow::Result<()> {
        anyhow::bail!("macOS screensaver support requires bundling as a .saver package")
    }
}
