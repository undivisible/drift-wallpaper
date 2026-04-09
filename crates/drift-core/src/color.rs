//! Colour palette management: built-in presets and image-based extraction.
//!
//! Drift-inspired presets (`FluxPlasma`, `FluxPoolside`, `FluxFreedom`) use three stops sampled
//! from the MIT-licensed colour wheels in [sandydoo/flux](https://github.com/sandydoo/flux)
//! (`flux/src/settings.rs`, `flux-gl/flux/src/settings.rs`). `FluxOriginal` is an approximate
//! macOS Drift–style cool gradient (not meant as a byte-identical match to Apple’s shader).

use image::DynamicImage;
use serde::{Deserialize, Serialize};

/// A named preset palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    /// Cool indigo → sky → pale cyan (Drift-like default accent).
    FluxOriginal,
    /// Stops sampled from the historical Flux Plasma wheel (indices 0, 2, 4).
    FluxPlasma,
    /// Stops sampled from the historical Flux Poolside wheel.
    FluxPoolside,
    /// Blue ↔ yellow emphasis from the historical Flux Freedom wheel.
    FluxFreedom,
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
            Self::FluxOriginal => "Original",
            Self::FluxPlasma => "Plasma",
            Self::FluxPoolside => "Poolside",
            Self::FluxFreedom => "Freedom",
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
            Self::FluxOriginal,
            Self::FluxPlasma,
            Self::FluxPoolside,
            Self::FluxFreedom,
            Self::Ocean,
            Self::Sunset,
            Self::Forest,
            Self::Lava,
            Self::Midnight,
            Self::Monochrome,
        ]
    }
}

/// One RGBA stop from Flux’s flat `[r,g,b,a, …]` colour wheel (`f32` 0–1).
#[inline]
fn flux_stop(wheel: &[f32], index: usize) -> [f32; 3] {
    let o = index * 4;
    [
        wheel[o].clamp(0.0, 1.0),
        wheel[o + 1].clamp(0.0, 1.0),
        wheel[o + 2].clamp(0.0, 1.0),
    ]
}

// Flux `COLOR_SCHEME_*` arrays from sandydoo/flux (MIT).
#[rustfmt::skip]
const FLUX_PLASMA: [f32; 24] = [
    60.219 / 255.0, 37.2487 / 255.0, 66.4301 / 255.0, 1.0,
    170.962 / 255.0, 54.4873 / 255.0, 50.9661 / 255.0, 1.0,
    230.299 / 255.0, 39.2759 / 255.0, 5.54531 / 255.0, 1.0,
    242.924 / 255.0, 94.3563 / 255.0, 22.4186 / 255.0, 1.0,
    242.435 / 255.0, 156.752 / 255.0, 58.9794 / 255.0, 1.0,
    135.291 / 255.0, 152.793 / 255.0, 182.473 / 255.0, 1.0,
];
#[rustfmt::skip]
const FLUX_POOLSIDE: [f32; 24] = [
    76.0 / 255.0, 156.0 / 255.0, 228.0 / 255.0, 1.0,
    140.0 / 255.0, 204.0 / 255.0, 244.0 / 255.0, 1.0,
    108.0 / 255.0, 180.0 / 255.0, 236.0 / 255.0, 1.0,
    188.0 / 255.0, 228.0 / 255.0, 244.0 / 255.0, 1.0,
    124.0 / 255.0, 220.0 / 255.0, 236.0 / 255.0, 1.0,
    156.0 / 255.0, 208.0 / 255.0, 236.0 / 255.0, 1.0,
];
#[rustfmt::skip]
const FLUX_FREEDOM: [f32; 24] = [
    0.0 / 255.0, 87.0 / 255.0, 183.0 / 255.0, 1.0,
    0.0 / 255.0, 87.0 / 255.0, 183.0 / 255.0, 1.0,
    0.0 / 255.0, 87.0 / 255.0, 183.0 / 255.0, 1.0,
    1.0, 215.0 / 255.0, 0.0, 1.0,
    1.0, 215.0 / 255.0, 0.0, 1.0,
    1.0, 215.0 / 255.0, 0.0, 1.0,
];

fn flux_palette_triple(wheel: &[f32; 24]) -> [[f32; 3]; 3] {
    [
        flux_stop(wheel, 0),
        flux_stop(wheel, 2),
        flux_stop(wheel, 4),
    ]
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
            Preset::FluxOriginal => [[0.02, 0.04, 0.18], [0.12, 0.38, 0.72], [0.85, 0.94, 0.98]],
            Preset::FluxPlasma => flux_palette_triple(&FLUX_PLASMA),
            Preset::FluxPoolside => flux_palette_triple(&FLUX_POOLSIDE),
            Preset::FluxFreedom => [
                flux_stop(&FLUX_FREEDOM, 0),
                flux_stop(&FLUX_FREEDOM, 2),
                flux_stop(&FLUX_FREEDOM, 4),
            ],
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

        let stops = refine_extracted_stops(median_cut_3(&pixels));
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

/// Re-order extracted stops dark → accent → highlight, then push chroma on the mid tone and
/// tame washed-out highlights so palettes read closer to “accent-led” than gray / white.
fn refine_extracted_stops(stops: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut v = [stops[0], stops[1], stops[2]];
    v.sort_by(|a, b| {
        relative_luminance_srgb(*a)
            .partial_cmp(&relative_luminance_srgb(*b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut dark = clamp3(v[0]);
    let accent = boost_accent_color(clamp3(v[1]));
    let mut light = clamp3(v[2]);

    let lum_light = relative_luminance_srgb(light);
    if lum_light > 0.82 {
        light = lerp3(light, accent, 0.38);
    }

    let lum_dark = relative_luminance_srgb(dark);
    if lum_dark > 0.55 {
        dark = lerp3(dark, accent, 0.22);
    }

    [dark, accent, clamp3(light)]
}

#[inline]
fn relative_luminance_srgb(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn boost_accent_color(rgb: [f32; 3]) -> [f32; 3] {
    let (h, s, l) = rgb_to_hsl(rgb);
    let s = (s * 1.5).min(1.0);
    let mut l = l;
    if l > 0.72 {
        l = l * 0.88 + 0.06;
    }
    if l < 0.1 {
        l = 0.1;
    }
    clamp3(hsl_to_rgb(h, s, l))
}

/// sRGB 0–1 → HSL with H in radians-style 0..2π for stability, S/L in 0–1.
fn rgb_to_hsl(rgb: [f32; 3]) -> (f32, f32, f32) {
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    let min = rgb[0].min(rgb[1]).min(rgb[2]);
    let d = max - min;
    let l = (max + min) * 0.5;

    if d <= 1e-6 {
        return (0.0, 0.0, l);
    }

    let s = if l >= 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };

    let mut h = if (max - rgb[0]).abs() < 1e-6 {
        (rgb[1] - rgb[2]) / d + if rgb[1] < rgb[2] { 6.0 } else { 0.0 }
    } else if (max - rgb[1]).abs() < 1e-6 {
        (rgb[2] - rgb[0]) / d + 2.0
    } else {
        (rgb[0] - rgb[1]) / d + 4.0
    };
    h /= 6.0;
    h *= std::f32::consts::TAU;
    (h, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0))
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    let h = (h / std::f32::consts::TAU).rem_euclid(1.0);
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;

    let r = hue_to_rgb(p, q, h + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h);
    let b = hue_to_rgb(p, q, h - 1.0 / 3.0);
    [r, g, b]
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    t = t.rem_euclid(1.0);
    if t < 1.0 / 6.0 {
        p + (q - p) * 6.0 * t
    } else if t < 0.5 {
        q
    } else if t < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
        p
    }
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
