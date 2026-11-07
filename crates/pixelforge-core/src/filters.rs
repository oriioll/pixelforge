//! Separable blurs, bilateral filtering and plane resampling.
//!
//! All planes are row-major `f32` slices of `w * h` values.

use rayon::prelude::*;

/// Gaussian blur approximated by three box blurs (error < 3% of a true gaussian).
pub fn gaussian(src: &[f32], w: usize, h: usize, sigma: f32) -> Vec<f32> {
    if sigma < 0.3 || w == 0 || h == 0 {
        return src.to_vec();
    }
    let mut a = src.to_vec();
    let mut b = vec![0.0; src.len()];
    for r in boxes_for_gauss(sigma, 3) {
        box_blur_h(&a, &mut b, w, r);
        box_blur_v(&b, &mut a, w, h, r);
    }
    a
}

/// Box radii whose repeated application approximates a gaussian of `sigma`.
fn boxes_for_gauss(sigma: f32, n: usize) -> Vec<usize> {
    let nf = n as f32;
    let ideal = (12.0 * sigma * sigma / nf + 1.0).sqrt();
    let mut wl = ideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let wlf = wl as f32;
    let m = ((12.0 * sigma * sigma - nf * wlf * wlf - 4.0 * nf * wlf - 3.0 * nf) / (-4.0 * wlf - 4.0)).round();
    (0..n).map(|i| (if (i as f32) < m { wl } else { wu }).max(1) as usize / 2).collect()
}

fn box_blur_h(src: &[f32], dst: &mut [f32], w: usize, r: usize) {
    let n = (2 * r + 1) as f32;
    dst.par_chunks_mut(w).zip(src.par_chunks(w)).for_each(|(d, s)| {
        let mut acc = s[0] * (r as f32 + 1.0);
        for i in 1..=r {
            acc += s[i.min(w - 1)];
        }
        for x in 0..w {
            d[x] = acc / n;
            acc += s[(x + r + 1).min(w - 1)] - s[x.saturating_sub(r)];
        }
    });
}

fn box_blur_v(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize) {
    let n = (2 * r + 1) as f32;
    let mut acc: Vec<f32> = src[..w].iter().map(|v| v * (r as f32 + 1.0)).collect();
    for i in 1..=r {
        let row = &src[i.min(h - 1) * w..][..w];
        acc.iter_mut().zip(row).for_each(|(a, v)| *a += v);
    }
    for y in 0..h {
        dst[y * w..][..w].iter_mut().zip(&acc).for_each(|(d, a)| *d = a / n);
        let add = &src[(y + r + 1).min(h - 1) * w..][..w];
        let sub = &src[y.saturating_sub(r) * w..][..w];
        for x in 0..w {
            acc[x] += add[x] - sub[x];
        }
    }
}

/// Bilateral filter on a plane (range weights taken from the same plane).
pub fn bilateral(src: &[f32], w: usize, h: usize, radius: usize, sigma_s: f32, sigma_r: f32) -> Vec<f32> {
    joint_bilateral(src, src, w, h, radius, sigma_s, sigma_r)
}

/// Joint bilateral filter: smooths `src` using range weights computed on `guide`.
/// Used to blur chroma without bleeding across luminance edges.
pub fn joint_bilateral(src: &[f32], guide: &[f32], w: usize, h: usize, radius: usize, sigma_s: f32, sigma_r: f32) -> Vec<f32> {
    const LUT: usize = 512;
    let max_d = 3.0 * sigma_r.max(1e-4);
    let range_lut: Vec<f32> = (0..LUT)
        .map(|i| {
            let d = i as f32 / (LUT - 1) as f32 * max_d;
            (-(d * d) / (2.0 * sigma_r * sigma_r)).exp()
        })
        .collect();
    let side = 2 * radius + 1;
    let spatial: Vec<f32> = (0..side * side)
        .map(|i| {
            let dx = (i % side) as f32 - radius as f32;
            let dy = (i / side) as f32 - radius as f32;
            (-(dx * dx + dy * dy) / (2.0 * sigma_s * sigma_s)).exp()
        })
        .collect();
    let scale = (LUT - 1) as f32 / max_d;
    let mut out = vec![0.0; src.len()];
    out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let g0 = guide[y * w + x];
            let (mut acc, mut wsum) = (0.0, 0.0);
            for ky in 0..side {
                let yy = (y + ky).saturating_sub(radius).min(h - 1);
                for kx in 0..side {
                    let xx = (x + kx).saturating_sub(radius).min(w - 1);
                    let idx = yy * w + xx;
                    let d = (guide[idx] - g0).abs();
                    if d >= max_d {
                        continue;
                    }
                    let wt = spatial[ky * side + kx] * range_lut[(d * scale) as usize];
                    acc += src[idx] * wt;
                    wsum += wt;
                }
            }
            row[x] = if wsum > 0.0 { acc / wsum } else { g0 };
        }
    });
    out
}

/// Averages `factor x factor` blocks. Returns the new plane and its size.
pub fn downscale_box(src: &[f32], w: usize, h: usize, factor: usize) -> (Vec<f32>, usize, usize) {
    if factor <= 1 {
        return (src.to_vec(), w, h);
    }
    let (nw, nh) = ((w / factor).max(1), (h / factor).max(1));
    let mut out = vec![0.0; nw * nh];
    out.par_chunks_mut(nw).enumerate().for_each(|(y, row)| {
        for x in 0..nw {
            let (mut acc, mut n) = (0.0, 0.0);
            for dy in 0..factor {
                for dx in 0..factor {
                    let (sx, sy) = (x * factor + dx, y * factor + dy);
                    if sx < w && sy < h {
                        acc += src[sy * w + sx];
                        n += 1.0;
                    }
                }
            }
            row[x] = acc / n;
        }
    });
    (out, nw, nh)
}

/// Bilinear resize of a plane (used to bring analysis masks back to full size).
pub fn resize_bilinear(src: &[f32], w: usize, h: usize, nw: usize, nh: usize) -> Vec<f32> {
    if (w, h) == (nw, nh) {
        return src.to_vec();
    }
    let (sx, sy) = (w as f32 / nw as f32, h as f32 / nh as f32);
    let mut out = vec![0.0; nw * nh];
    out.par_chunks_mut(nw).enumerate().for_each(|(y, row)| {
        let fy = ((y as f32 + 0.5) * sy - 0.5).clamp(0.0, (h - 1) as f32);
        let (y0, ty) = (fy.floor() as usize, fy.fract());
        let y1 = (y0 + 1).min(h - 1);
        for x in 0..nw {
            let fx = ((x as f32 + 0.5) * sx - 0.5).clamp(0.0, (w - 1) as f32);
            let (x0, tx) = (fx.floor() as usize, fx.fract());
            let x1 = (x0 + 1).min(w - 1);
            let top = src[y0 * w + x0] * (1.0 - tx) + src[y0 * w + x1] * tx;
            let bot = src[y1 * w + x0] * (1.0 - tx) + src[y1 * w + x1] * tx;
            row[x] = top * (1.0 - ty) + bot * ty;
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaussian_keeps_constant_planes() {
        let p = vec![0.4; 40 * 30];
        let b = gaussian(&p, 40, 30, 3.0);
        assert!(b.iter().all(|v| (v - 0.4).abs() < 1e-4));
    }

    #[test]
    fn gaussian_preserves_energy_of_an_impulse() {
        let (w, h) = (64, 64);
        let mut p = vec![0.0; w * h];
        p[32 * w + 32] = 1.0;
        let b = gaussian(&p, w, h, 2.5);
        let sum: f32 = b.iter().sum();
        assert!((sum - 1.0).abs() < 1e-3, "sum was {sum}");
        assert!(b[32 * w + 32] < 0.1);
    }

    #[test]
    fn bilateral_denoises_flat_areas_but_keeps_the_step() {
        let (w, h) = (48, 24);
        let mut p = vec![0.0; w * h];
        let mut seed = 12345u32;
        for y in 0..h {
            for x in 0..w {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let noise = ((seed >> 16) as f32 / 65535.0 - 0.5) * 0.04;
                p[y * w + x] = if x < w / 2 { 0.2 } else { 0.8 } + noise;
            }
        }
        let out = bilateral(&p, w, h, 2, 1.5, 0.06);
        let var = |plane: &[f32], x0: usize, x1: usize| {
            let vals: Vec<f32> = (0..h).flat_map(|y| (x0..x1).map(move |x| (y, x))).map(|(y, x)| plane[y * w + x]).collect();
            let m = vals.iter().sum::<f32>() / vals.len() as f32;
            vals.iter().map(|v| (v - m).powi(2)).sum::<f32>() / vals.len() as f32
        };
        assert!(var(&out, 4, 20) < var(&p, 4, 20) * 0.5);
        assert!(out[12 * w + 20] < 0.3 && out[12 * w + 27] > 0.7, "the step must survive");
    }

    #[test]
    fn resize_round_trip_is_close() {
        let (w, h) = (16, 16);
        let p: Vec<f32> = (0..w * h).map(|i| (i % w) as f32 / w as f32).collect();
        let up = resize_bilinear(&p, w, h, 32, 32);
        let down = resize_bilinear(&up, 32, 32, w, h);
        assert!(p.iter().zip(&down).all(|(a, b)| (a - b).abs() < 0.05));
    }

    #[test]
    fn downscale_box_averages() {
        let p = vec![1.0, 3.0, 5.0, 7.0];
        let (o, w, h) = downscale_box(&p, 2, 2, 2);
        assert_eq!((w, h), (1, 1));
        assert_eq!(o[0], 4.0);
    }
}
