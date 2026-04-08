//! Colour palette management: built-in presets and image-based extraction.

use image::DynamicImage;
use serde::{Deserialize, Serialize};

/// A named preset palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    Ocean,
    Sunset,
    Forest,
    Lava,
    Midnight,
    Monochrome,
}

impl Preset {
    /// Return a short human-readable label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Ocean => "Ocean",
            Self::Sunset => "Sunset",
            Self::Forest => "Forest",
            Self::Lava => "Lava",
            Self::Midnight => "Midnight",
            Self::Monochrome => "Monochrome",
        }
    }

    /// All available presets, in display order.
    pub fn all() -> &'static [Preset] {
        &[
            Self::Ocean,
            Self::Sunset,
            Self::Forest,
            Self::Lava,
            Self::Midnight,
            Self::Monochrome,
        ]
    }
}

/// A three-stop colour palette used by the Drift renderer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColorPalette {
    /// Three RGB stops in linear sRGB (each component 0–1).
    stops: [[f32; 3]; 3],
    /// Optional user label.
    pub label: Option<String>,
}

impl ColorPalette {
    /// Create a palette from a named preset.
    pub fn preset(p: Preset) -> Self {
        let stops = match p {
            Preset::Ocean => [[0.02, 0.04, 0.25], [0.03, 0.35, 0.65], [0.60, 0.95, 0.98]],
            Preset::Sunset => [[0.10, 0.03, 0.20], [0.80, 0.25, 0.10], [0.99, 0.85, 0.30]],
            Preset::Forest => [[0.02, 0.12, 0.04], [0.10, 0.45, 0.10], [0.70, 0.90, 0.30]],
            Preset::Lava => [[0.08, 0.01, 0.01], [0.60, 0.10, 0.02], [0.99, 0.65, 0.05]],
            Preset::Midnight => [[0.01, 0.01, 0.08], [0.05, 0.05, 0.30], [0.35, 0.35, 0.90]],
            Preset::Monochrome => [[0.02, 0.02, 0.02], [0.40, 0.40, 0.40], [0.95, 0.95, 0.95]],
        };
        Self {
            stops,
            label: Some(p.label().to_owned()),
        }
    }

    /// Create a palette from three explicit RGB stops (values clamped to 0–1).
    pub fn from_stops(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Self {
        Self {
            stops: [clamp3(a), clamp3(b), clamp3(c)],
            label: None,
        }
    }

    /// Extract a three-stop palette from an image using median-cut colour
    /// quantisation.  Falls back to the [`Preset::Midnight`] palette if the
    /// image is empty.
    pub fn from_image(img: &DynamicImage) -> Self {
        let rgb = img.to_rgb8();
        let pixels: Vec<[f32; 3]> = rgb
            .pixels()
            .map(|p| {
                [
                    (p[0] as f32) / 255.0,
                    (p[1] as f32) / 255.0,
                    (p[2] as f32) / 255.0,
                ]
            })
            .collect();

        if pixels.is_empty() {
            return Self::preset(Preset::Midnight);
        }

        let stops = median_cut_3(&pixels);
        Self {
            stops,
            label: Some("Custom (image)".to_owned()),
        }
    }

    /// Return the three colour stops as a slice.
    pub fn colors(&self) -> [[f32; 3]; 3] {
        self.stops
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn clamp3(c: [f32; 3]) -> [f32; 3] {
    [
        c[0].clamp(0.0, 1.0),
        c[1].clamp(0.0, 1.0),
        c[2].clamp(0.0, 1.0),
    ]
}

/// Very lightweight median-cut: partition the pixel list into three clusters
/// by the channel with the widest range, then average each cluster.
fn median_cut_3(pixels: &[[f32; 3]]) -> [[f32; 3]; 3] {
    // First split: divide along the widest channel.
    let (channel0, _) = widest_channel(pixels);
    let mut sorted = pixels.to_vec();
    sorted.sort_by(|a, b| a[channel0].partial_cmp(&b[channel0]).unwrap());
    let mid0 = sorted.len() / 2;
    let (left, right) = sorted.split_at(mid0);

    // Second split: pick whichever half has the wider range.
    let (channel_l, range_l) = widest_channel(left);
    let (channel_r, range_r) = widest_channel(right);
    let (a, b, c) = if range_l >= range_r {
        let mut l2 = left.to_vec();
        l2.sort_by(|a, b| a[channel_l].partial_cmp(&b[channel_l]).unwrap());
        let mid1 = l2.len() / 2;
        (average(&l2[..mid1]), average(&l2[mid1..]), average(right))
    } else {
        let mut r2 = right.to_vec();
        r2.sort_by(|a, b| a[channel_r].partial_cmp(&b[channel_r]).unwrap());
        let mid1 = r2.len() / 2;
        (average(left), average(&r2[..mid1]), average(&r2[mid1..]))
    };

    [a, b, c]
}

fn widest_channel(pixels: &[[f32; 3]]) -> (usize, f32) {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for p in pixels {
        for c in 0..3 {
            min[c] = min[c].min(p[c]);
            max[c] = max[c].max(p[c]);
        }
    }
    let ranges = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    let ch = if ranges[0] >= ranges[1] && ranges[0] >= ranges[2] {
        0
    } else if ranges[1] >= ranges[2] {
        1
    } else {
        2
    };
    (ch, ranges[ch])
}

fn average(pixels: &[[f32; 3]]) -> [f32; 3] {
    if pixels.is_empty() {
        return [0.0; 3];
    }
    let n = pixels.len() as f32;
    let sum = pixels.iter().fold([0.0f32; 3], |acc, p| {
        [acc[0] + p[0], acc[1] + p[1], acc[2] + p[2]]
    });
    [sum[0] / n, sum[1] / n, sum[2] / n]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_colors_in_range() {
        for &preset in Preset::all() {
            let palette = ColorPalette::preset(preset);
            for stop in palette.colors() {
                for v in stop {
                    assert!(
                        (0.0..=1.0).contains(&v),
                        "colour component out of range in preset {:?}: {v}",
                        preset
                    );
                }
            }
        }
    }

    #[test]
    fn from_stops_clamps() {
        let p = ColorPalette::from_stops([-1.0, 0.5, 2.0], [0.5; 3], [0.5; 3]);
        let colors = p.colors();
        assert_eq!(colors[0][0], 0.0);
        assert_eq!(colors[0][1], 0.5);
        assert_eq!(colors[0][2], 1.0);
    }

    #[test]
    fn from_image_returns_three_stops() {
        // Create a tiny 4×4 test image with two dominant colours.
        let mut img = image::RgbImage::new(4, 4);
        for (x, _y, pixel) in img.enumerate_pixels_mut() {
            *pixel = if x < 2 {
                image::Rgb([200u8, 50, 50])
            } else {
                image::Rgb([50u8, 50, 200])
            };
        }
        let dyn_img = image::DynamicImage::ImageRgb8(img);
        let palette = ColorPalette::from_image(&dyn_img);
        assert_eq!(palette.colors().len(), 3);
    }

    #[test]
    fn all_presets_have_labels() {
        for &preset in Preset::all() {
            assert!(!preset.label().is_empty());
        }
    }

    #[test]
    fn palette_roundtrip_json() {
        let p = ColorPalette::preset(Preset::Ocean);
        let json = serde_json::to_string(&p).unwrap();
        let p2: ColorPalette = serde_json::from_str(&json).unwrap();
        assert_eq!(p, p2);
    }
}
