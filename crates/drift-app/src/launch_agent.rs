//! macOS Launch Agent management.
//!
//! Installs / removes a per-user `launchd` plist so that the drift wallpaper
//! starts automatically at login.  On non-macOS platforms the functions are
//! no-ops.
// Items in this module are used only from the macOS-specific code path.
#![allow(dead_code)]

use anyhow::Result;

/// Identifier used for the Launch Agent.
pub const LAUNCH_AGENT_LABEL: &str = "com.drift-wallpaper.macos";

/// Plist content template – the `{BIN}` placeholder is replaced at runtime
/// with the absolute path to the current executable.
const PLIST_TEMPLATE: &str = include_str!("../assets/com.drift-wallpaper.macos.plist");

/// Install the Launch Agent plist for the current user.
pub fn install() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let bin = std::env::current_exe()?.to_string_lossy().into_owned();
        let plist = PLIST_TEMPLATE.replace("{BIN}", &bin);
        let dest = agent_path()?;
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&dest, plist)?;
        // Ask launchd to load the agent immediately.
        let _ = std::process::Command::new("launchctl")
            .args(["load", "-w", dest.to_str().unwrap_or("")])
            .status();
        log::info!("Launch Agent installed at {dest:?}");
    }
    Ok(())
}

/// Remove the Launch Agent plist for the current user.
pub fn uninstall() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let dest = agent_path()?;
        if dest.exists() {
            let _ = std::process::Command::new("launchctl")
                .args(["unload", "-w", dest.to_str().unwrap_or("")])
                .status();
            std::fs::remove_file(&dest)?;
            log::info!("Launch Agent removed from {dest:?}");
        }
    }
    Ok(())
}

/// Whether the Launch Agent is currently installed.
pub fn is_installed() -> bool {
    #[cfg(target_os = "macos")]
    {
        agent_path().map(|p| p.exists()).unwrap_or(false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Path to `~/Library/LaunchAgents/<label>.plist`.
#[cfg(target_os = "macos")]
fn agent_path() -> Result<std::path::PathBuf> {
    let home = std::env::var("HOME")?;
    Ok(std::path::PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{LAUNCH_AGENT_LABEL}.plist")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_template_contains_placeholder() {
        assert!(
            PLIST_TEMPLATE.contains("{BIN}"),
            "plist template must contain the {{BIN}} placeholder"
        );
    }

    #[test]
    fn plist_template_contains_label() {
        assert!(
            PLIST_TEMPLATE.contains(LAUNCH_AGENT_LABEL),
            "plist template must contain the label '{LAUNCH_AGENT_LABEL}'"
        );
    }
}
