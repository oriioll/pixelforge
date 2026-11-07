//! Mask-driven sharpening and detail re-injection.

use crate::buffer::RgbF;
use crate::filters::gaussian;
use rayon::prelude::*;

/// Luma unsharp mask whose strength is a per-pixel plane.
///
/// Only luminance is modified (no color fringes) and the correction is soft-clipped with
/// `tanh` so strong edges do not grow halos or burn highlights.
pub fn unsharp_luma(img: &mut RgbF, sigma: f32, amount: &[f32], clip: f32) {
    let luma = img.luma();
    let blur = gaussian(&luma, img.w, img.h, sigma);
    img.data.par_chunks_exact_mut(3).enumerate().for_each(|(i, px)| {
        let a = amount[i];
        if a <= 0.0 {
            return;
        }
        let delta = (luma[i] - blur[i]) * a;
        let delta = clip * (delta / clip).tanh();
        px.iter_mut().for_each(|c| *c = (*c + delta).clamp(0.0, 1.0));
    });
}

/// Adds back fine detail from `reference` (same size) where `amount` is high.
/// Counteracts the waxy look that super-resolution gives to skin.
pub fn reinject_detail(img: &mut RgbF, reference: &RgbF, sigma: f32, amount: &[f32]) {
    debug_assert_eq!((img.w, img.h), (reference.w, reference.h));
    let rl = reference.luma();
    let rb = gaussian(&rl, reference.w, reference.h, sigma);
    img.data.par_chunks_exact_mut(3).enumerate().for_each(|(i, px)| {
        let d = (rl[i] - rb[i]) * amount[i];
        px.iter_mut().for_each(|c| *c = (*c + d).clamp(0.0, 1.0));
    });
}

/// Constant plane.
pub fn plane(w: usize, h: usize, v: f32) -> Vec<f32> {
    vec![v; w * h]
}

/// Gaussian blobs of influence around `(x, y, radius)` spots, max-combined.
/// Used to target the eyes without touching the rest of the face.
pub fn spot_plane(w: usize, h: usize, spots: &[(f32, f32, f32)]) -> Vec<f32> {
    let mut out = vec![0.0f32; w * h];
    for &(cx, cy, r) in spots {
        let s = (r / 1.5).max(1.0);
        let (x0, x1) = ((cx - 3.0 * s).max(0.0) as usize, ((cx + 3.0 * s) as usize + 1).min(w));
        let (y0, y1) = ((cy - 3.0 * s).max(0.0) as usize, ((cy + 3.0 * s) as usize + 1).min(h));
        for y in y0..y1 {
            for x in x0..x1 {
                let d2 = (x as f32 - cx).powi(2) + (y as f32 - cy).powi(2);
                let v = (-d2 / (2.0 * s * s)).exp();
                let o = &mut out[y * w + x];
                *o = o.max(v);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step_image(w: usize, h: usize) -> RgbF {
        let mut img = RgbF::new(w, h);
        for (i, p) in img.data.chunks_exact_mut(3).enumerate() {
            let v = if i % w < w / 2 { 0.3 } else { 0.6 };
            p.copy_from_slice(&[v, v, v]);
        }
        img
    }

    #[test]
    fn unsharp_increases_edge_contrast() {
        let mut img = step_image(60, 20);
        let amount = plane(60, 20, 1.5);
        unsharp_luma(&mut img, 1.5, &amount, 0.2);
        let (dark_side, bright_side) = (img.data[(10 * 60 + 28) * 3], img.data[(10 * 60 + 31) * 3]);
        assert!(dark_side < 0.3 && bright_side > 0.6);
    }

    #[test]
    fn zero_amount_is_a_no_op() {
        let mut img = step_image(60, 20);
        let before = img.data.clone();
        unsharp_luma(&mut img, 1.5, &plane(60, 20, 0.0), 0.2);
        assert_eq!(img.data, before);
    }

    #[test]
    fn halos_are_bounded_by_the_clip() {
        let mut img = step_image(60, 20);
        let before = img.data.clone();
        unsharp_luma(&mut img, 1.5, &plane(60, 20, 10.0), 0.1);
        assert!(img.data.iter().zip(&before).all(|(a, b)| (a - b).abs() <= 0.1 + 1e-6));
    }

    #[test]
    fn spot_plane_peaks_at_the_center() {
        let p = spot_plane(50, 50, &[(25.0, 25.0, 6.0)]);
        assert!(p[25 * 50 + 25] > 0.99 && p[5 * 50 + 5] < 0.01);
    }
}
