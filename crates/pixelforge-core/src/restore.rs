//! Source restoration that runs *before* upscaling: JPEG block removal and gentle denoise.
//!
//! Real-ESRGAN amplifies whatever it is given, so blocking from messenger recompression
//! (Telegram, WhatsApp) is cleaned first.

use crate::buffer::{luma_of, RgbF, LUMA_B, LUMA_G, LUMA_R};
use crate::filters::{bilateral, joint_bilateral};

/// Minimum ratio between the strongest 8-px grid phase and the average of the others
/// for the image to be considered "blocky" on that axis.
const BLOCKINESS_RATIO: f32 = 1.12;

/// Finds the offset (0..8) of the JPEG 8x8 grid on each axis, if one is detectable.
///
/// Messenger apps resize first and compress after, so the grid is usually at offset 0,
/// but cropped screenshots shift it. Returns `None` for an axis without a visible grid.
pub fn detect_block_phase(luma: &[f32], w: usize, h: usize) -> (Option<usize>, Option<usize>) {
    if w < 32 || h < 32 {
        return (None, None);
    }
    let mut cols = [0.0f64; 8];
    let mut col_n = [0u32; 8];
    for y in (0..h).step_by(2) {
        let row = &luma[y * w..][..w];
        for x in 1..w {
            cols[x % 8] += (row[x] - row[x - 1]).abs() as f64;
            col_n[x % 8] += 1;
        }
    }
    let mut rows = [0.0f64; 8];
    let mut row_n = [0u32; 8];
    for y in 1..h {
        let (a, b) = (&luma[(y - 1) * w..][..w], &luma[y * w..][..w]);
        for x in (0..w).step_by(2) {
            rows[y % 8] += (b[x] - a[x]).abs() as f64;
            row_n[y % 8] += 1;
        }
    }
    (pick_phase(&cols, &col_n), pick_phase(&rows, &row_n))
}

fn pick_phase(sum: &[f64; 8], n: &[u32; 8]) -> Option<usize> {
    let mean: Vec<f64> = (0..8).map(|i| sum[i] / n[i].max(1) as f64).collect();
    let (best, &peak) = mean.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
    let others = (mean.iter().sum::<f64>() - peak) / 7.0;
    // A perfectly flat neighbourhood (others ~ 0) with a visible peak is the clearest grid of all.
    (peak > 1e-4 && peak / others.max(1e-6) > BLOCKINESS_RATIO as f64).then_some(best)
}

/// Smooths luma across 8-px block boundaries when the step looks like a codec artifact
/// (small step on a flat neighbourhood) and leaves real edges untouched.
pub fn deblock_luma(luma: &mut [f32], w: usize, h: usize, strength: f32, phase: (Option<usize>, Option<usize>)) {
    let s = strength.clamp(0.0, 1.0);
    if s == 0.0 {
        return;
    }
    let thr = 0.02 + 0.10 * s;
    if let Some(px) = phase.0 {
        for x in (px.max(8)..w.saturating_sub(2)).step_by(8) {
            for y in 0..h {
                let i = y * w + x;
                smooth_edge(luma, i - 2, i - 1, i, i + 1, s, thr);
            }
        }
    }
    if let Some(py) = phase.1 {
        for y in (py.max(8)..h.saturating_sub(2)).step_by(8) {
            for x in 0..w {
                let i = y * w + x;
                smooth_edge(luma, i - 2 * w, i - w, i, i + w, s, thr);
            }
        }
    }
}

#[inline]
fn smooth_edge(v: &mut [f32], i_p1: usize, i_p0: usize, i_q0: usize, i_q1: usize, s: f32, thr: f32) {
    let (p1, p0, q0, q1) = (v[i_p1], v[i_p0], v[i_q0], v[i_q1]);
    let d = q0 - p0;
    if d.abs() < thr && (p0 - p1).abs() < thr && (q1 - q0).abs() < thr {
        v[i_p0] += d * 0.25 * s;
        v[i_q0] -= d * 0.25 * s;
        v[i_p1] += d * 0.125 * s;
        v[i_q1] -= d * 0.125 * s;
    }
}

/// Parameters of the pre-upscale restoration.
#[derive(Clone, Copy, Debug)]
pub struct RestoreParams {
    pub deblock: f32,
    pub denoise_luma: f32,
    pub denoise_chroma: f32,
}

/// Removes blocking and noise in place. Chroma is filtered harder than luma because
/// compression damages color first and the eye forgives it less in flat areas.
pub fn restore_source(img: &mut RgbF, p: RestoreParams) {
    let (w, h) = (img.w, img.h);
    let n = w * h;
    let mut luma = img.luma();
    let mut cb = vec![0.0; n];
    let mut cr = vec![0.0; n];
    for (i, px) in img.data.chunks_exact(3).enumerate() {
        cb[i] = px[2] - luma[i];
        cr[i] = px[0] - luma[i];
    }

    if p.deblock > 0.0 {
        let phase = detect_block_phase(&luma, w, h);
        deblock_luma(&mut luma, w, h, p.deblock, phase);
    }
    if p.denoise_chroma > 0.0 {
        let sr = 0.03 + 0.07 * p.denoise_chroma.clamp(0.0, 1.0);
        cb = joint_bilateral(&cb, &luma, w, h, 3, 2.0, sr);
        cr = joint_bilateral(&cr, &luma, w, h, 3, 2.0, sr);
    }
    if p.denoise_luma > 0.0 {
        luma = bilateral(&luma, w, h, 2, 1.5, 0.03 + 0.08 * p.denoise_luma.clamp(0.0, 1.0));
    }

    for (i, px) in img.data.chunks_exact_mut(3).enumerate() {
        let (r, b) = (luma[i] + cr[i], luma[i] + cb[i]);
        let g = (luma[i] - LUMA_R * r - LUMA_B * b) / LUMA_G;
        px[0] = r.clamp(0.0, 1.0);
        px[1] = g.clamp(0.0, 1.0);
        px[2] = b.clamp(0.0, 1.0);
    }
    debug_assert!((luma_of(0.5, 0.5, 0.5) - 0.5).abs() < 1e-5);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Image of flat 8x8 blocks with a different DC level each, grid shifted by (ox, oy).
    fn blocky(w: usize, h: usize, ox: usize, oy: usize) -> Vec<f32> {
        let mut seed = 7u32;
        let (bw, bh) = (w / 8 + 2, h / 8 + 2);
        let levels: Vec<f32> = (0..bw * bh)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                0.5 + ((seed >> 16) as f32 / 65535.0 - 0.5) * 0.06
            })
            .collect();
        (0..w * h).map(|i| levels[((i / w + 8 - oy) / 8) * bw + (i % w + 8 - ox) / 8]).collect()
    }

    fn boundary_energy(p: &[f32], w: usize, h: usize, ox: usize) -> f32 {
        let mut s = 0.0;
        for y in 0..h {
            for x in (ox.max(8)..w).step_by(8) {
                s += (p[y * w + x] - p[y * w + x - 1]).abs();
            }
        }
        s
    }

    #[test]
    fn finds_the_shifted_grid() {
        let (w, h) = (128, 128);
        let p = blocky(w, h, 3, 5);
        assert_eq!(detect_block_phase(&p, w, h), (Some(3), Some(5)));
    }

    #[test]
    fn ignores_images_without_a_grid() {
        let (w, h) = (128, 128);
        let p: Vec<f32> = (0..w * h).map(|i| ((i % w) as f32 * 0.37).sin() * 0.1 + 0.5).collect();
        assert_eq!(detect_block_phase(&p, w, h).0, None);
    }

    #[test]
    fn deblocking_reduces_boundary_steps() {
        let (w, h) = (128, 128);
        let mut p = blocky(w, h, 0, 0);
        let before = boundary_energy(&p, w, h, 0);
        deblock_luma(&mut p, w, h, 1.0, (Some(0), Some(0)));
        assert!(boundary_energy(&p, w, h, 0) < before * 0.7);
    }

    #[test]
    fn real_edges_are_not_smoothed() {
        let (w, h) = (64, 64);
        let mut p: Vec<f32> = (0..w * h).map(|i| if i % w < 32 { 0.1 } else { 0.9 }).collect();
        let copy = p.clone();
        deblock_luma(&mut p, w, h, 1.0, (Some(0), None));
        assert_eq!(p, copy);
    }

    #[test]
    fn restore_keeps_flat_color() {
        let mut img = RgbF::new(40, 40);
        img.data.chunks_exact_mut(3).for_each(|p| p.copy_from_slice(&[0.2, 0.5, 0.7]));
        restore_source(&mut img, RestoreParams { deblock: 1.0, denoise_luma: 0.5, denoise_chroma: 1.0 });
        assert!(img.data.chunks_exact(3).all(|p| (p[0] - 0.2).abs() < 1e-3 && (p[1] - 0.5).abs() < 1e-3 && (p[2] - 0.7).abs() < 1e-3));
    }
}
