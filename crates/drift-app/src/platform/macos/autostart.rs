//! macOS autostart via launchd.

use crate::platform::AutostartManager;

pub struct MacosAutostart;

impl AutostartManager for MacosAutostart {
    fn install() -> anyhow::Result<()> {
        crate::launch_agent::install()
    }

    fn uninstall() -> anyhow::Result<()> {
        crate::launch_agent::uninstall()
    }

    fn is_installed() -> bool {
        crate::launch_agent::is_installed()
    }
}
