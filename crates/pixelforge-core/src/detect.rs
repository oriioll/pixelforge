//! Cheap preset suggestion from the picture itself (no model required).

use crate::buffer::RgbF;
use crate::masks::grass_mask;
use crate::presets::Preset;
use image::{imageops, RgbImage};
use serde::Serialize;

/// Preset the UI should pre-select, with a confidence and a machine-readable reason.
#[derive(Clone, Debug, Serialize)]
pub struct Suggestion {
    pub preset: Preset,
    /// 0..1. Below ~0.5 the UI should present it as a guess.
    pub confidence: f32,
    /// `"pitch_detected"`, `"no_pitch_portrait"` or `"no_pitch"`; the UI turns it into text.
    pub reason: &'static str,
}

/// Share of the lower 70% of the frame that must be grass-green to call it a football photo.
const PITCH_RATIO: f32 = 0.10;

/// Looks for a pitch in the lower part of the frame; anything else is treated as a selfie.
pub fn suggest(img: &RgbImage) -> Suggestion {
    let (w, h) = img.dimensions();
    let scale = 256.0 / w.max(h) as f32;
    let small = imageops::thumbnail(img, ((w as f32 * scale).round() as u32).max(1), ((h as f32 * scale).round() as u32).max(1));
    let f = RgbF::from_rgb8(&small);
    let mask = grass_mask(&f);
    let start = (f.h as f32 * 0.3) as usize * f.w;
    let region = &mask[start..];
    let ratio = region.iter().filter(|&&m| m > 0.5).count() as f32 / region.len().max(1) as f32;
    if ratio >= PITCH_RATIO {
        Suggestion { preset: Preset::Football, confidence: (0.5 + ratio * 1.5).min(1.0), reason: "pitch_detected" }
    } else if h as f32 / w as f32 >= 1.3 {
        Suggestion { preset: Preset::MirrorSelfie, confidence: 0.55, reason: "no_pitch_portrait" }
    } else {
        Suggestion { preset: Preset::MirrorSelfie, confidence: 0.4, reason: "no_pitch" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn green_lower_half_is_football() {
        let img = RgbImage::from_fn(200, 300, |_, y| if y < 100 { Rgb([20, 20, 40]) } else { Rgb([50, 140, 50]) });
        let s = suggest(&img);
        assert_eq!(s.preset, Preset::Football);
        assert!(s.confidence > 0.7);
    }

    #[test]
    fn indoor_portrait_is_a_selfie() {
        let img = RgbImage::from_fn(200, 300, |_, _| Rgb([200, 170, 150]));
        let s = suggest(&img);
        assert_eq!((s.preset, s.reason), (Preset::MirrorSelfie, "no_pitch_portrait"));
    }
}
