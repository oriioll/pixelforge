//! Soft masks (0..1) that decide *where* each enhancement is applied.

use crate::buffer::{percentile, smoothstep, RgbF};
use crate::filters::{downscale_box, gaussian, resize_bilinear};

/// "In focus" mask from local sharpness. Bright where the image has real detail
/// (player, kit, crest) and dark in blur (crowd bokeh, motion blur), so those areas
/// are left alone instead of getting invented texture.
///
/// Computed on a ~720 px analysis copy and resized back.
pub fn focus_mask(luma: &[f32], w: usize, h: usize) -> Vec<f32> {
    let factor = (w.max(h) as f32 / 720.0).ceil().max(1.0) as usize;
    let (small, sw, sh) = downscale_box(luma, w, h, factor);
    let mut lap = vec![0.0; sw * sh];
    for y in 1..sh.saturating_sub(1) {
        for x in 1..sw.saturating_sub(1) {
            let c = small[y * sw + x];
            lap[y * sw + x] = (4.0 * c - small[y * sw + x - 1] - small[y * sw + x + 1] - small[(y - 1) * sw + x] - small[(y + 1) * sw + x]).abs();
        }
    }
    let energy = gaussian(&lap, sw, sh, (sw.max(sh) as f32 * 0.012).max(2.0));
    let p95 = percentile(&energy, 0.95).max(1e-5);
    let mask: Vec<f32> = energy.iter().map(|e| smoothstep(0.12, 0.6, e / p95)).collect();
    let mask = gaussian(&mask, sw, sh, 3.0);
    resize_bilinear(&mask, sw, sh, w, h)
}

/// Soft membership of "grass green" pixels (hue 65-170 deg, enough saturation).
pub fn grass_mask(img: &RgbF) -> Vec<f32> {
    let raw: Vec<f32> = img
        .data
        .chunks_exact(3)
        .map(|p| {
            let (r, g, b) = (p[0], p[1], p[2]);
            let max = r.max(g).max(b);
            let delta = max - r.min(g).min(b);
            if max < 0.1 || delta <= 1e-4 || (g - max).abs() > 1e-6 {
                return 0.0;
            }
            let hue = 60.0 * (2.0 + (b - r) / delta);
            let sat = delta / max;
            smoothstep(55.0, 80.0, hue) * (1.0 - smoothstep(160.0, 185.0, hue)) * smoothstep(0.15, 0.3, sat)
        })
        .collect();
    gaussian(&raw, img.w, img.h, 2.0)
}

/// Soft skin membership using the classic YCbCr box (Cb 77-127, Cr 133-173).
pub fn skin_mask(img: &RgbF) -> Vec<f32> {
    let raw: Vec<f32> = img
        .data
        .chunks_exact(3)
        .map(|p| {
            let (r, g, b) = (p[0], p[1], p[2]);
            let y = 0.299 * r + 0.587 * g + 0.114 * b;
            let cb = 128.0 + (-0.168736 * r - 0.331264 * g + 0.5 * b) * 255.0;
            let cr = 128.0 + (0.5 * r - 0.418688 * g - 0.081312 * b) * 255.0;
            let in_cb = smoothstep(72.0, 82.0, cb) * (1.0 - smoothstep(122.0, 132.0, cb));
            let in_cr = smoothstep(128.0, 138.0, cr) * (1.0 - smoothstep(168.0, 178.0, cr));
            in_cb * in_cr * smoothstep(0.12, 0.2, y)
        })
        .collect();
    gaussian(&raw, img.w, img.h, 3.0)
}

/// Edge gate from Sobel magnitude: 1 on real edges, 0 on flat or smooth areas.
pub fn edge_gate(luma: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut mag = vec![0.0; w * h];
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let at = |dx: isize, dy: isize| luma[(y as isize + dy) as usize * w + (x as isize + dx) as usize];
            let gx = -at(-1, -1) - 2.0 * at(-1, 0) - at(-1, 1) + at(1, -1) + 2.0 * at(1, 0) + at(1, 1);
            let gy = -at(-1, -1) - 2.0 * at(0, -1) - at(1, -1) + at(-1, 1) + 2.0 * at(0, 1) + at(1, 1);
            mag[y * w + x] = (gx * gx + gy * gy).sqrt();
        }
    }
    let p98 = percentile(&mag, 0.98).max(1e-5);
    mag.iter().map(|m| smoothstep(0.05, 0.35, m / p98)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker_left_flat_right(w: usize, h: usize) -> Vec<f32> {
        (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                if x < w / 2 { if (x / 2 + y / 2) % 2 == 0 { 0.2 } else { 0.8 } } else { 0.5 }
            })
            .collect()
    }

    #[test]
    fn focus_mask_prefers_detailed_regions() {
        let (w, h) = (400, 300);
        let m = focus_mask(&checker_left_flat_right(w, h), w, h);
        assert!(m[150 * w + 80] > 0.7, "detailed side: {}", m[150 * w + 80]);
        assert!(m[150 * w + 330] < 0.2, "flat side: {}", m[150 * w + 330]);
    }

    #[test]
    fn grass_mask_detects_green_only() {
        let mut img = RgbF::new(40, 20);
        for (i, p) in img.data.chunks_exact_mut(3).enumerate() {
            p.copy_from_slice(if i % 40 < 20 { &[0.2, 0.55, 0.2] } else { &[0.7, 0.2, 0.2] });
        }
        let m = grass_mask(&img);
        assert!(m[10 * 40 + 5] > 0.8);
        assert!(m[10 * 40 + 35] < 0.1);
    }

    #[test]
    fn skin_mask_detects_skin_tones() {
        let mut img = RgbF::new(30, 30);
        img.data.chunks_exact_mut(3).for_each(|p| p.copy_from_slice(&[0.85, 0.65, 0.55]));
        assert!(skin_mask(&img)[15 * 30 + 15] > 0.8);
        img.data.chunks_exact_mut(3).for_each(|p| p.copy_from_slice(&[0.2, 0.3, 0.8]));
        assert!(skin_mask(&img)[15 * 30 + 15] < 0.1);
    }

    #[test]
    fn edge_gate_fires_on_edges_only() {
        let (w, h) = (40, 40);
        let l: Vec<f32> = (0..w * h).map(|i| if i % w < 20 { 0.1 } else { 0.9 }).collect();
        let g = edge_gate(&l, w, h);
        assert!(g[20 * w + 20] > 0.9);
        assert!(g[20 * w + 5] < 0.05);
    }
}
