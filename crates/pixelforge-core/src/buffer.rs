//! Floating point RGB buffer used by every processing stage.

use image::{RgbImage, Rgb};

/// Rec. 709 luma weights. All luminance-driven stages use the same ones.
pub const LUMA_R: f32 = 0.2126;
pub const LUMA_G: f32 = 0.7152;
pub const LUMA_B: f32 = 0.0722;

/// Interleaved RGB image with channels in `0.0..=1.0`.
#[derive(Clone, Debug)]
pub struct RgbF {
    pub w: usize,
    pub h: usize,
    pub data: Vec<f32>,
}

impl RgbF {
    /// Creates a black image.
    pub fn new(w: usize, h: usize) -> Self {
        Self { w, h, data: vec![0.0; w * h * 3] }
    }

    /// Converts an 8-bit image into the float buffer.
    pub fn from_rgb8(img: &RgbImage) -> Self {
        let data = img.as_raw().iter().map(|&v| v as f32 / 255.0).collect();
        Self { w: img.width() as usize, h: img.height() as usize, data }
    }

    /// Converts back to 8-bit, clamping and rounding.
    pub fn to_rgb8(&self) -> RgbImage {
        let raw: Vec<u8> = self.data.iter().map(|&v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8).collect();
        RgbImage::from_raw(self.w as u32, self.h as u32, raw).expect("buffer size matches dimensions")
    }

    /// Per-pixel luma plane.
    pub fn luma(&self) -> Vec<f32> {
        self.data.chunks_exact(3).map(|p| luma_of(p[0], p[1], p[2])).collect()
    }

    /// Replaces every pixel with `f(r, g, b)`.
    pub fn map_pixels(&mut self, f: impl Fn([f32; 3]) -> [f32; 3] + Sync + Send) {
        use rayon::prelude::*;
        self.data.par_chunks_exact_mut(3).for_each(|p| {
            let o = f([p[0], p[1], p[2]]);
            p.copy_from_slice(&o);
        });
    }
}

/// Luma of a single pixel.
#[inline]
pub fn luma_of(r: f32, g: f32, b: f32) -> f32 {
    LUMA_R * r + LUMA_G * g + LUMA_B * b
}

/// Smooth Hermite step between `e0` and `e1`.
#[inline]
pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Linear interpolation.
#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Returns the value at quantile `q` (0..1) of a plane. Sorts a copy.
pub fn percentile(plane: &[f32], q: f32) -> f32 {
    if plane.is_empty() {
        return 0.0;
    }
    let mut v: Vec<f32> = plane.to_vec();
    let idx = ((v.len() - 1) as f32 * q.clamp(0.0, 1.0)) as usize;
    let (_, nth, _) = v.select_nth_unstable_by(idx, |a, b| a.total_cmp(b));
    *nth
}

/// Re-exported so callers can build pixels without importing `image`.
pub fn rgb8(r: u8, g: u8, b: u8) -> Rgb<u8> {
    Rgb([r, g, b])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_is_lossless_for_8bit() {
        let img = RgbImage::from_fn(5, 4, |x, y| Rgb([(x * 40) as u8, (y * 60) as u8, 255 - (x * y) as u8]));
        assert_eq!(RgbF::from_rgb8(&img).to_rgb8(), img);
    }

    #[test]
    fn percentile_of_ramp() {
        let ramp: Vec<f32> = (0..101).map(|i| i as f32).collect();
        assert_eq!(percentile(&ramp, 0.5), 50.0);
        assert_eq!(percentile(&ramp, 1.0), 100.0);
    }
}
