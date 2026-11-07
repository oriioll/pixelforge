//! Tonal corrections applied after upscaling: shadows/highlights, white balance, levels.

use crate::buffer::{luma_of, percentile, smoothstep, RgbF};
use crate::filters::gaussian;

/// Lifts shadows and tames highlights using a *local* brightness estimate, so a dark sky
/// stays dark overall while detail inside shadows (kits, faces) is recovered.
///
/// `shadows` and `highlights` are 0..1. Gain is capped so noise is not amplified.
pub fn shadows_highlights(img: &mut RgbF, shadows: f32, highlights: f32) {
    if shadows <= 0.0 && highlights <= 0.0 {
        return;
    }
    let (w, h) = (img.w, img.h);
    let luma = img.luma();
    let base = gaussian(&luma, w, h, (w.max(h) as f32 / 80.0).max(2.0));
    let gains: Vec<f32> = (0..w * h)
        .map(|i| {
            let (l, b) = (luma[i], base[i]);
            let lift = shadows * 0.5 * (1.0 - b).powi(2) * (1.0 - l) * smoothstep(0.0, 0.1, l);
            let knee = 0.78;
            let drop = highlights * 0.4 * smoothstep(knee, 1.0, l) * (l - knee).max(0.0) * (0.4 + 0.6 * smoothstep(0.6, 0.95, b));
            ((l + lift - drop) / l.max(1e-3)).clamp(0.5, 2.2)
        })
        .collect();
    for (px, g) in img.data.chunks_exact_mut(3).zip(gains) {
        px.iter_mut().for_each(|c| *c = (*c * g).min(1.0));
    }
}

/// Gray-world white balance restricted to mid-tones with low saturation, so a red kit
/// or green pitch does not drag the estimate. `strength` blends toward the full correction.
pub fn white_balance(img: &mut RgbF, strength: f32) {
    let s = strength.clamp(0.0, 1.0);
    if s == 0.0 {
        return;
    }
    let (mut sum, mut n) = ([0.0f64; 3], 0u64);
    for px in img.data.chunks_exact(3).step_by(3) {
        let l = luma_of(px[0], px[1], px[2]);
        let sat = px[0].max(px[1]).max(px[2]) - px[0].min(px[1]).min(px[2]);
        if (0.12..0.88).contains(&l) && sat < 0.35 {
            (0..3).for_each(|c| sum[c] += px[c] as f64);
            n += 1;
        }
    }
    if n < 200 {
        return;
    }
    let mean = [sum[0] / n as f64, sum[1] / n as f64, sum[2] / n as f64];
    let target = (mean[0] + mean[1] + mean[2]) / 3.0;
    let gain: [f32; 3] = std::array::from_fn(|c| {
        let full = (target / mean[c].max(1e-4)) as f32;
        1.0 + (full.clamp(0.85, 1.2) - 1.0) * s
    });
    img.map_pixels(move |p| [(p[0] * gain[0]).min(1.0), (p[1] * gain[1]).min(1.0), (p[2] * gain[2]).min(1.0)]);
}

/// Gentle percentile stretch (0.5% / 99.5%) with a hard cap on the added contrast.
pub fn auto_levels(img: &mut RgbF, strength: f32) {
    let s = strength.clamp(0.0, 1.0);
    if s == 0.0 {
        return;
    }
    let luma = img.luma();
    let (lo, hi) = (percentile(&luma, 0.005), percentile(&luma, 0.995));
    if hi - lo < 0.2 {
        return;
    }
    let lo_e = lo * s;
    let hi_e = 1.0 - (1.0 - hi) * s;
    let scale = (1.0 / (hi_e - lo_e)).clamp(1.0, 1.15);
    img.map_pixels(move |p| [((p[0] - lo_e) * scale).clamp(0.0, 1.0), ((p[1] - lo_e) * scale).clamp(0.0, 1.0), ((p[2] - lo_e) * scale).clamp(0.0, 1.0)]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: usize, h: usize, rgb: [f32; 3]) -> RgbF {
        let mut i = RgbF::new(w, h);
        i.data.chunks_exact_mut(3).for_each(|p| p.copy_from_slice(&rgb));
        i
    }

    #[test]
    fn shadows_get_brighter_and_mid_tones_barely_move() {
        let mut dark = flat(64, 64, [0.08, 0.08, 0.08]);
        shadows_highlights(&mut dark, 0.6, 0.0);
        assert!(dark.data[0] > 0.12);
        let mut mid = flat(64, 64, [0.6, 0.6, 0.6]);
        shadows_highlights(&mut mid, 0.6, 0.0);
        assert!((mid.data[0] - 0.6).abs() < 0.05);
    }

    #[test]
    fn highlights_are_pulled_down_in_bright_regions() {
        let mut bright = flat(64, 64, [0.99, 0.99, 0.99]);
        shadows_highlights(&mut bright, 0.0, 1.0);
        assert!(bright.data[0] < 0.97);
    }

    #[test]
    fn white_balance_neutralizes_a_cast() {
        let mut img = flat(64, 64, [0.62, 0.5, 0.4]);
        white_balance(&mut img, 1.0);
        let (r, b) = (img.data[0], img.data[2]);
        assert!((r - b).abs() < 0.2, "cast reduced: r={r} b={b}");
    }

    #[test]
    fn black_pixels_stay_black() {
        let mut img = flat(32, 32, [0.0, 0.0, 0.0]);
        shadows_highlights(&mut img, 1.0, 1.0);
        assert!(img.data.iter().all(|&v| v == 0.0));
    }
}
