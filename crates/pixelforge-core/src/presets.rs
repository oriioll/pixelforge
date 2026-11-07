//! The two enhancement presets and how the intensity slider scales them.

use serde::{Deserialize, Serialize};

/// Scene-specific tuning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    /// Vertical football photos: pitch, rain, dark sky, stadium lights, kits.
    Football,
    /// Mirror selfies: skin, eyes, text, clothes and reflections.
    MirrorSelfie,
}

/// Slider value that produces the preset's nominal look.
pub const NOMINAL_INTENSITY: f32 = 0.6;

/// Every tunable of the pipeline, already scaled by intensity.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    // Pre-upscale restoration
    pub deblock: f32,
    pub denoise_luma: f32,
    pub denoise_chroma: f32,
    // Tone
    pub shadows: f32,
    pub highlights: f32,
    pub white_balance: f32,
    pub auto_levels: f32,
    // Selective sharpening (amounts for the unsharp mask)
    pub sharpen_focus: f32,
    pub sharpen_blur: f32,
    pub sharpen_grass: f32,
    /// How much sharpening is restricted to real edges (0 = everywhere, 1 = edges only).
    pub edge_gate: f32,
    /// Minimum share of the AI result kept in out-of-focus areas.
    /// Lower values fall back to a plain resize there, which respects bokeh.
    pub bokeh_floor: f32,
    pub final_sharpen: f32,
    /// Sharpening multiplier on skin (1 = same as elsewhere).
    pub skin_sharpen: f32,
    // Faces
    pub texture_reinject: f32,
    pub eye_sharpen: f32,
    /// Default CodeFormer fidelity (higher keeps the original face more faithfully).
    pub face_fidelity: f32,
}

impl Preset {
    /// Resolves the preset at `intensity` (0..1). Artifact removal does not scale with
    /// intensity; everything that changes the look does.
    pub fn resolve(self, intensity: f32) -> Params {
        let k = (intensity.clamp(0.0, 1.0) / NOMINAL_INTENSITY).min(1.67);
        match self {
            Preset::Football => Params {
                deblock: 0.8,
                denoise_luma: 0.25 * k, // light: rain and sky grain are part of the picture
                denoise_chroma: 0.6,
                shadows: 0.22 * k,
                highlights: 0.5 * k,
                white_balance: 0.0, // never recolor kits
                auto_levels: 0.0,
                sharpen_focus: 0.9 * k,
                sharpen_blur: 0.08 * k,
                sharpen_grass: 0.6 * k,
                edge_gate: 0.6,
                bokeh_floor: 0.35,
                final_sharpen: 0.25 * k,
                skin_sharpen: 1.0,
                texture_reinject: 0.0,
                eye_sharpen: 0.0,
                face_fidelity: 0.8,
            },
            Preset::MirrorSelfie => Params {
                deblock: 0.6,
                denoise_luma: 0.35 * k,
                denoise_chroma: 0.8,
                shadows: 0.18 * k,
                highlights: 0.25 * k,
                white_balance: (0.7 * k).min(0.85),
                auto_levels: 0.5,
                sharpen_focus: 0.55 * k,
                sharpen_blur: 0.25 * k, // text, clothes and reflections sit everywhere in frame
                sharpen_grass: 0.0,
                edge_gate: 0.5,
                bokeh_floor: 0.8,
                final_sharpen: 0.2 * k,
                skin_sharpen: 0.5,
                texture_reinject: 0.35,
                eye_sharpen: 0.7 * k,
                face_fidelity: 0.9,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn more_intensity_means_stronger_sharpening() {
        for p in [Preset::Football, Preset::MirrorSelfie] {
            let (lo, hi) = (p.resolve(0.2), p.resolve(0.9));
            assert!(hi.sharpen_focus > lo.sharpen_focus && hi.final_sharpen > lo.final_sharpen);
        }
    }

    #[test]
    fn artifact_removal_does_not_depend_on_intensity() {
        assert_eq!(Preset::Football.resolve(0.1).deblock, Preset::Football.resolve(1.0).deblock);
    }

    #[test]
    fn zero_intensity_applies_no_look_changes() {
        let p = Preset::MirrorSelfie.resolve(0.0);
        assert_eq!((p.sharpen_focus, p.shadows, p.eye_sharpen), (0.0, 0.0, 0.0));
    }

    #[test]
    fn football_respects_bokeh_more_than_selfie() {
        assert!(Preset::Football.resolve(0.6).bokeh_floor < Preset::MirrorSelfie.resolve(0.6).bokeh_floor);
        assert!(Preset::Football.resolve(0.6).sharpen_blur < Preset::MirrorSelfie.resolve(0.6).sharpen_blur);
    }

    #[test]
    fn serializes_as_snake_case() {
        assert_eq!(serde_json::to_string(&Preset::MirrorSelfie).unwrap(), "\"mirror_selfie\"");
    }
}
