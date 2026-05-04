//! Windows autostart via Registry.
//!
//! Uses the Windows Registry Run key to implement login item functionality.

use crate::platform::AutostartManager;

pub struct WindowsAutostart;

impl AutostartManager for WindowsAutostart {
    fn install() -> anyhow::Result<()> {
        use windows::Win32::System::Registry::*;

        unsafe {
            let exe_path = std::env::current_exe()?;
            let exe_str = exe_path.to_string_lossy();

            let hkcu = HKEY_CURRENT_USER;
            let run_key = windows::core::w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");

            let mut key: HKEY = std::mem::zeroed();
            RegOpenKeyExW(hkcu, run_key, 0, KEY_WRITE, &mut key)?;

            let value_name = windows::core::w!("DriftWallpaper");
            let data = windows::core::PWSTR(exe_str.as_ptr() as *mut u16);

            RegSetValueExW(
                key,
                value_name,
                0,
                REG_SZ,
                Some(std::slice::from_raw_parts(
                    data.0 as *const u8,
                    exe_str.len() * 2,
                )),
            )?;

            RegCloseKey(key)?;
        }

        log::info!("Windows autostart installed via Registry");
        Ok(())
    }

    fn uninstall() -> anyhow::Result<()> {
        use windows::Win32::System::Registry::*;

        unsafe {
            let hkcu = HKEY_CURRENT_USER;
            let run_key = windows::core::w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");

            let mut key: HKEY = std::mem::zeroed();
            RegOpenKeyExW(hkcu, run_key, 0, KEY_WRITE, &mut key)?;

            let value_name = windows::core::w!("DriftWallpaper");
            let _ = RegDeleteValueW(key, value_name);

            RegCloseKey(key)?;
        }

        log::info!("Windows autostart removed from Registry");
        Ok(())
    }

    fn is_installed() -> bool {
        use windows::Win32::System::Registry::*;

        unsafe {
            let hkcu = HKEY_CURRENT_USER;
            let run_key = windows::core::w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");

            let mut key: HKEY = std::mem::zeroed();
            if RegOpenKeyExW(hkcu, run_key, 0, KEY_READ, &mut key) != 0 {
                return false;
            }

            let value_name = windows::core::w!("DriftWallpaper");
            let mut data: [u16; 260] = [0; 260];
            let mut data_size = (data.len() * 2) as u32;
            let mut value_type: u32 = 0;

            let result = RegQueryValueExW(
                key,
                value_name,
                None,
                Some(&mut value_type),
                Some(&mut data),
                Some(&mut data_size),
            );

            let _ = RegCloseKey(key);

            result == 0 && value_type == REG_SZ
        }
    }
}
