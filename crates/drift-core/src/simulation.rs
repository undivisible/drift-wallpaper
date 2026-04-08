//! Simulation parameters that drive the Drift fluid aesthetic.

use serde::{Deserialize, Serialize};

/// All tunable parameters for the Drift simulation.
///
/// These are serialised to / deserialised from the user config file so that
/// preferences survive app restarts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DriftParams {
    /// Overall animation speed multiplier (default: 1.0).
    pub speed: f32,
    /// Spatial zoom / scale of the noise field (default: 1.0).
    pub scale: f32,
    /// First colour stop of the gradient (linear sRGB, 0–1).
    pub color_a: [f32; 3],
    /// Second colour stop (linear sRGB, 0–1).
    pub color_b: [f32; 3],
    /// Third colour stop (linear sRGB, 0–1).
    pub color_c: [f32; 3],
    /// Target frames per second (default: 60).
    pub target_fps: u32,
}

impl Default for DriftParams {
    fn default() -> Self {
        Self {
            speed: 1.0,
            scale: 1.0,
            color_a: [0.09, 0.05, 0.29], // deep violet
            color_b: [0.20, 0.60, 0.86], // sky blue
            color_c: [0.98, 0.80, 0.40], // warm gold
            target_fps: 60,
        }
    }
}

impl DriftParams {
    /// Create params from a [`crate::color::ColorPalette`].
    pub fn from_palette(palette: &crate::color::ColorPalette, speed: f32) -> Self {
        let colors = palette.colors();
        let get = |i: usize| {
            if i < colors.len() {
                colors[i]
            } else {
                colors[colors.len() - 1]
            }
        };
        Self {
            speed,
            color_a: get(0),
            color_b: get(1),
            color_c: get(2),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_params_are_valid() {
        let p = DriftParams::default();
        assert!(p.speed > 0.0);
        assert!(p.scale > 0.0);
        assert!(p.target_fps > 0);
        for c in [p.color_a, p.color_b, p.color_c] {
            for v in c {
                assert!(
                    (0.0..=1.0).contains(&v),
                    "colour component out of range: {v}"
                );
            }
        }
    }

    #[test]
    fn params_roundtrip_json() {
        let p = DriftParams::default();
        let json = serde_json::to_string(&p).expect("serialise");
        let p2: DriftParams = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(p, p2);
    }

    #[test]
    fn from_palette_maps_colors() {
        use crate::color::ColorPalette;
        let palette = ColorPalette::preset(crate::color::Preset::Ocean);
        let params = DriftParams::from_palette(&palette, 1.5);
        assert_eq!(params.speed, 1.5);
        let colors = palette.colors();
        assert_eq!(params.color_a, colors[0]);
        assert_eq!(params.color_b, colors[1]);
        assert_eq!(params.color_c, colors[2]);
    }
}
