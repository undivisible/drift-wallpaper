//! Windows native color picker via ChooseColor dialog.

use crate::platform::NativeColorPicker;

pub struct WindowsColorPicker;

impl WindowsColorPicker {
    pub fn pick_color(initial: Option<[u8; 3]>) -> anyhow::Result<Option<[u8; 3]>> {
        use windows::Win32::Graphics::Gdi::*;
        use windows::Win32::UI::WindowsAndMessaging::*;

        unsafe {
            let mut cc = CHOOSECOLORW::default();
            cc.lStructSize = std::mem::size_of::<CHOOSECOLORW>() as u32;
            cc.hwndOwner = HWND_DESKTOP;

            let (r, g, b) = if let Some([r, g, b]) = initial {
                (r as u32, g as u32, b as u32)
            } else {
                (0xFF, 0xFF, 0xFF)
            };

            let mut custom_colors = [0u32; 16];
            cc.lpCustColors = custom_colors.as_mut_ptr();

            cc.rgbResult = RGB(r, g, b);
            cc.Flags = CC_RGBINIT | CC_FULLOPEN | CC_ENABLEHOOK;

            if ChooseColorW(&mut cc) {
                let color = cc.rgbResult;
                let r = (color & 0xFF) as u8;
                let g = ((color >> 8) & 0xFF) as u8;
                let b = ((color >> 16) & 0xFF) as u8;
                Ok(Some([r, g, b]))
            } else {
                Ok(None)
            }
        }
    }
}

impl NativeColorPicker for WindowsColorPicker {
    fn pick_color(initial: Option<[u8; 3]>) -> anyhow::Result<Option<[u8; 3]>> {
        Self::pick_color(initial)
    }
}
