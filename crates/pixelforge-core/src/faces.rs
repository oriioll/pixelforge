//! Face detection (SCRFD) and restoration (CodeFormer).
//!
//! Geometry, anchor decoding and blending are plain Rust and unit-tested. Only the two
//! ONNX Runtime calls live behind the `codeformer` feature.

use crate::buffer::{smoothstep, RgbF};
use crate::error::{Error, Result};
use crate::filters::{downscale_box, resize_bilinear};
use std::path::Path;

pub type Pt = [f32; 2];
/// 2x3 affine matrix `[a, b, tx, c, d, ty]`: `(x, y) -> (a x + b y + tx, c x + d y + ty)`.
pub type Affine = [f32; 6];

pub const DETECTOR_FILE: &str = "scrfd_2.5g.onnx";
pub const CODEFORMER_FILE: &str = "codeformer.onnx";
/// Input resolution of the SCRFD detector.
pub const DET_SIZE: usize = 640;
/// CodeFormer works on aligned 512x512 crops.
pub const CROP: usize = 512;
/// Faces narrower than this (in output pixels) are left alone: restoring them hallucinates detail.
pub const MIN_FACE_PX: f32 = 32.0;

/// Five-point FFHQ template (left eye, right eye, nose, left mouth, right mouth) in 0..1 units.
const FFHQ_TEMPLATE: [Pt; 5] = [
    [0.37691676, 0.46864664],
    [0.62285697, 0.46912813],
    [0.50123859, 0.61331904],
    [0.39308822, 0.72541100],
    [0.61150205, 0.72490465],
];

/// A detected face.
#[derive(Clone, Debug)]
pub struct Face {
    /// `[x1, y1, x2, y2]` in image pixels.
    pub bbox: [f32; 4],
    pub score: f32,
    /// Left eye, right eye, nose, left mouth corner, right mouth corner.
    pub landmarks: [Pt; 5],
}

impl Face {
    pub fn width(&self) -> f32 {
        self.bbox[2] - self.bbox[0]
    }

    /// `(x, y, radius)` of both eyes, radius being a fraction of the inter-ocular distance.
    pub fn eye_spots(&self) -> [(f32, f32, f32); 2] {
        let [l, r] = [self.landmarks[0], self.landmarks[1]];
        let eye_r = ((r[0] - l[0]).powi(2) + (r[1] - l[1]).powi(2)).sqrt() * 0.22;
        [(l[0], l[1], eye_r), (r[0], r[1], eye_r)]
    }
}

/// Least-squares similarity transform (rotation, uniform scale, translation) mapping `src` onto `dst`.
pub fn estimate_similarity(src: &[Pt; 5], dst: &[Pt; 5]) -> Affine {
    let n = 5.0;
    let mean = |p: &[Pt; 5]| (p.iter().map(|q| q[0]).sum::<f32>() / n, p.iter().map(|q| q[1]).sum::<f32>() / n);
    let ((sx, sy), (dx, dy)) = (mean(src), mean(dst));
    let (mut num_a, mut num_b, mut den) = (0.0, 0.0, 0.0);
    for i in 0..5 {
        let (x, y) = (src[i][0] - sx, src[i][1] - sy);
        let (u, v) = (dst[i][0] - dx, dst[i][1] - dy);
        num_a += x * u + y * v;
        num_b += x * v - y * u;
        den += x * x + y * y;
    }
    let (a, b) = (num_a / den.max(1e-9), num_b / den.max(1e-9));
    [a, -b, dx - (a * sx - b * sy), b, a, dy - (b * sx + a * sy)]
}

/// Inverse of an affine transform.
pub fn invert(m: &Affine) -> Affine {
    let det = m[0] * m[4] - m[1] * m[3];
    let (a, b, c, d) = (m[4] / det, -m[1] / det, -m[3] / det, m[0] / det);
    [a, b, -(a * m[2] + b * m[5]), c, d, -(c * m[2] + d * m[5])]
}

#[inline]
pub fn apply(m: &Affine, x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5])
}

/// Transform taking image coordinates to the aligned 512x512 crop of `face`.
pub fn crop_transform(face: &Face) -> Affine {
    let dst = FFHQ_TEMPLATE.map(|p| [p[0] * CROP as f32, p[1] * CROP as f32]);
    estimate_similarity(&face.landmarks, &dst)
}

fn sample(img: &RgbF, x: f32, y: f32) -> [f32; 3] {
    let (xf, yf) = (x.clamp(0.0, (img.w - 1) as f32), y.clamp(0.0, (img.h - 1) as f32));
    let (x0, y0) = (xf.floor() as usize, yf.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(img.w - 1), (y0 + 1).min(img.h - 1));
    let (tx, ty) = (xf - x0 as f32, yf - y0 as f32);
    std::array::from_fn(|c| {
        let at = |xx: usize, yy: usize| img.data[(yy * img.w + xx) * 3 + c];
        (at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx) * (1.0 - ty) + (at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx) * ty
    })
}

/// Samples the aligned `CROP x CROP` face crop out of `img`.
pub fn extract_crop(img: &RgbF, to_crop: &Affine) -> RgbF {
    let inv = invert(to_crop);
    let mut out = RgbF::new(CROP, CROP);
    for v in 0..CROP {
        for u in 0..CROP {
            let (x, y) = apply(&inv, u as f32, v as f32);
            out.data[(v * CROP + u) * 3..][..3].copy_from_slice(&sample(img, x, y));
        }
    }
    out
}

/// Blend weight of the restored crop at crop coordinates `(u, v)`: an ellipse around the face
/// with a soft edge, so only face pixels are replaced and the background keeps its detail.
pub fn face_blend_weight(u: f32, v: f32) -> f32 {
    let s = CROP as f32;
    let r = (((u - 0.5 * s) / (0.42 * s)).powi(2) + ((v - 0.55 * s) / (0.48 * s)).powi(2)).sqrt();
    1.0 - smoothstep(0.75, 1.0, r)
}

/// Pastes the restored crop back into `img` through the inverse alignment.
pub fn paste_back(img: &mut RgbF, restored: &RgbF, to_crop: &Affine) {
    let inv = invert(to_crop);
    let corners = [(0.0, 0.0), (CROP as f32, 0.0), (0.0, CROP as f32), (CROP as f32, CROP as f32)].map(|(u, v)| apply(&inv, u, v));
    let min = |f: fn(&(f32, f32)) -> f32| corners.iter().map(f).fold(f32::MAX, f32::min);
    let max = |f: fn(&(f32, f32)) -> f32| corners.iter().map(f).fold(f32::MIN, f32::max);
    let (x0, x1) = (min(|p| p.0).floor().max(0.0) as usize, (max(|p| p.0).ceil().max(0.0) as usize).min(img.w));
    let (y0, y1) = (min(|p| p.1).floor().max(0.0) as usize, (max(|p| p.1).ceil().max(0.0) as usize).min(img.h));
    for y in y0..y1 {
        for x in x0..x1 {
            let (u, v) = apply(to_crop, x as f32, y as f32);
            if u < 0.0 || v < 0.0 || u > (CROP - 1) as f32 || v > (CROP - 1) as f32 {
                continue;
            }
            let wgt = face_blend_weight(u, v);
            if wgt <= 0.0 {
                continue;
            }
            let new = sample(restored, u, v);
            let dst = &mut img.data[(y * img.w + x) * 3..][..3];
            for c in 0..3 {
                dst[c] += (new[c] - dst[c]) * wgt;
            }
        }
    }
}

/// Greedy non-maximum suppression by IoU.
pub fn nms(mut faces: Vec<Face>, iou_threshold: f32) -> Vec<Face> {
    faces.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<Face> = vec![];
    for f in faces {
        if kept.iter().all(|k| iou(&k.bbox, &f.bbox) < iou_threshold) {
            kept.push(f);
        }
    }
    kept
}

fn iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let (ix, iy) = ((a[2].min(b[2]) - a[0].max(b[0])).max(0.0), (a[3].min(b[3]) - a[1].max(b[1])).max(0.0));
    let inter = ix * iy;
    let union = (a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}

/// Decodes the nine SCRFD outputs (scores, boxes, keypoints for strides 8/16/32) into faces
/// in *detector input* coordinates. Two anchors per location, row-major.
pub fn decode_scrfd(scores: [&[f32]; 3], boxes: [&[f32]; 3], kps: [&[f32]; 3], size: usize, threshold: f32) -> Vec<Face> {
    let mut faces = vec![];
    for (level, stride) in [8usize, 16, 32].into_iter().enumerate() {
        let fm = size / stride;
        let s = stride as f32;
        for idx in 0..fm * fm * 2 {
            let score = scores[level][idx];
            if score < threshold {
                continue;
            }
            let loc = idx / 2;
            let (ax, ay) = ((loc % fm) as f32 * s, (loc / fm) as f32 * s);
            let d = &boxes[level][idx * 4..][..4];
            let k = &kps[level][idx * 10..][..10];
            faces.push(Face {
                bbox: [ax - d[0] * s, ay - d[1] * s, ax + d[2] * s, ay + d[3] * s],
                score,
                landmarks: std::array::from_fn(|i| [ax + k[2 * i] * s, ay + k[2 * i + 1] * s]),
            });
        }
    }
    nms(faces, 0.4)
}

/// Builds the detector's CHW input: long side scaled to `size`, zero padded at the bottom/right,
/// normalized as `(v * 255 - 127.5) / 128`. Returns the tensor data and the scale factor used.
pub fn prepare_detector_input(img: &RgbF, size: usize) -> (Vec<f32>, f32) {
    let scale = size as f32 / img.w.max(img.h) as f32;
    let (nw, nh) = (((img.w as f32 * scale).round() as usize).clamp(1, size), ((img.h as f32 * scale).round() as usize).clamp(1, size));
    let mut out = vec![(0.0 - 127.5) / 128.0; 3 * size * size];
    for c in 0..3 {
        let plane: Vec<f32> = img.data.chunks_exact(3).map(|p| p[c]).collect();
        let factor = (1.0 / scale).floor().max(1.0) as usize;
        let (pre, pw, ph) = downscale_box(&plane, img.w, img.h, factor);
        let small = resize_bilinear(&pre, pw, ph, nw, nh);
        for y in 0..nh {
            for x in 0..nw {
                out[c * size * size + y * size + x] = (small[y * nw + x] * 255.0 - 127.5) / 128.0;
            }
        }
    }
    (out, scale)
}

/// Face models available in `dir` (checks the files, not whether ONNX Runtime was compiled in).
pub fn models_present(dir: &Path) -> bool {
    dir.join(DETECTOR_FILE).is_file() && dir.join(CODEFORMER_FILE).is_file()
}

/// True when this build can run CodeFormer.
pub const fn compiled_in() -> bool {
    cfg!(feature = "codeformer")
}

#[cfg(feature = "codeformer")]
mod onnx {
    use super::*;
    use ort::{inputs, session::{builder::GraphOptimizationLevel, Session}, value::Tensor};

    fn fe(e: impl std::fmt::Display) -> Error {
        Error::Faces(e.to_string())
    }

    /// SCRFD detector + CodeFormer, both on CPU through ONNX Runtime.
    pub struct FaceEngine {
        detector: Session,
        codeformer: Session,
    }

    impl FaceEngine {
        /// Loads both models from `dir`.
        ///
        /// # Errors
        /// [`Error::Faces`] if a file is missing or ONNX Runtime rejects it.
        pub fn load(dir: &Path) -> Result<Self> {
            let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
            let build = |file: &str| -> Result<Session> {
                Session::builder()
                    .and_then(|b| b.with_optimization_level(GraphOptimizationLevel::Level3))
                    .and_then(|b| b.with_intra_threads(threads))
                    .and_then(|b| b.commit_from_file(dir.join(file)))
                    .map_err(fe)
            };
            Ok(Self { detector: build(DETECTOR_FILE)?, codeformer: build(CODEFORMER_FILE)? })
        }

        /// Detects faces in `img` (coordinates are in `img` pixels).
        pub fn detect(&mut self, img: &RgbF) -> Result<Vec<Face>> {
            let (input, scale) = prepare_detector_input(img, DET_SIZE);
            let tensor = Tensor::from_array(([1usize, 3, DET_SIZE, DET_SIZE], input)).map_err(fe)?;
            let name = self.detector.inputs[0].name.clone();
            let outputs = self.detector.run(inputs![name => tensor]).map_err(fe)?;
            let get = |i: usize| -> Result<&[f32]> { outputs[i].try_extract_tensor::<f32>().map(|(_, d)| d).map_err(fe) };
            // Output order of the facefusion SCRFD export: 3 scores, 3 boxes, 3 keypoints.
            let faces = decode_scrfd([get(0)?, get(1)?, get(2)?], [get(3)?, get(4)?, get(5)?], [get(6)?, get(7)?, get(8)?], DET_SIZE, 0.5);
            Ok(faces
                .into_iter()
                .map(|mut f| {
                    f.bbox.iter_mut().for_each(|v| *v /= scale);
                    f.landmarks.iter_mut().for_each(|p| p.iter_mut().for_each(|v| *v /= scale));
                    f
                })
                .collect())
        }

        /// Runs CodeFormer on an aligned 512x512 crop. `fidelity` 0..1: higher keeps the
        /// original face closer (less hallucination).
        pub fn restore_crop(&mut self, crop: &RgbF, fidelity: f32) -> Result<RgbF> {
            let plane = CROP * CROP;
            let mut chw = vec![0.0f32; 3 * plane];
            for (i, p) in crop.data.chunks_exact(3).enumerate() {
                for c in 0..3 {
                    chw[c * plane + i] = (p[c] - 0.5) / 0.5;
                }
            }
            let input = Tensor::from_array(([1usize, 3, CROP, CROP], chw)).map_err(fe)?;
            // The exported graph declares `weight` as a rank-0 double (checked against codeformer.onnx).
            let weight = Tensor::from_array(((), vec![fidelity.clamp(0.0, 1.0) as f64])).map_err(fe)?;
            let outputs = self.codeformer.run(inputs!["input" => input, "weight" => weight]).map_err(fe)?;
            let (_, data) = outputs["output"].try_extract_tensor::<f32>().map_err(fe)?;
            if data.len() != 3 * plane {
                return Err(Error::Faces(format!("unexpected CodeFormer output size {}", data.len())));
            }
            let mut out = RgbF::new(CROP, CROP);
            for i in 0..plane {
                for c in 0..3 {
                    out.data[i * 3 + c] = ((data[c * plane + i] + 1.0) * 0.5).clamp(0.0, 1.0);
                }
            }
            Ok(out)
        }
    }
}

#[cfg(not(feature = "codeformer"))]
mod onnx {
    use super::*;

    /// Placeholder used when the crate is built without the `codeformer` feature.
    pub struct FaceEngine;

    impl FaceEngine {
        pub fn load(_: &Path) -> Result<Self> {
            Err(Error::Faces("this build does not include CodeFormer".into()))
        }
        pub fn detect(&mut self, _: &RgbF) -> Result<Vec<Face>> {
            Ok(vec![])
        }
        pub fn restore_crop(&mut self, crop: &RgbF, _: f32) -> Result<RgbF> {
            Ok(crop.clone())
        }
    }
}

pub use onnx::FaceEngine;

/// Restores every usable face in `img` in place and returns all detected faces
/// (callers reuse them for eye sharpening).
///
/// # Errors
/// Propagates [`Error::Faces`] from the models.
pub fn restore_faces(engine: &mut FaceEngine, img: &mut RgbF, faces: &[Face], fidelity: f32) -> Result<()> {
    for face in faces.iter().filter(|f| f.width() >= MIN_FACE_PX) {
        let to_crop = crop_transform(face);
        let crop = extract_crop(img, &to_crop);
        let restored = engine.restore_crop(&crop, fidelity)?;
        paste_back(img, &restored, &to_crop);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template_face(scale: f32, tx: f32, ty: f32) -> Face {
        let lm = FFHQ_TEMPLATE.map(|p| [p[0] * 512.0 * scale + tx, p[1] * 512.0 * scale + ty]);
        Face { bbox: [lm[0][0], lm[0][1], lm[1][0], lm[3][1]], score: 0.9, landmarks: lm }
    }

    #[test]
    fn similarity_recovers_rotation_scale_and_translation() {
        let src: [Pt; 5] = [[10.0, 20.0], [50.0, 22.0], [30.0, 40.0], [18.0, 60.0], [44.0, 61.0]];
        let (s, ang) = (1.7f32, 0.35f32);
        let dst = src.map(|p| [s * (ang.cos() * p[0] - ang.sin() * p[1]) + 5.0, s * (ang.sin() * p[0] + ang.cos() * p[1]) - 9.0]);
        let m = estimate_similarity(&src, &dst);
        for i in 0..5 {
            let (x, y) = apply(&m, src[i][0], src[i][1]);
            assert!((x - dst[i][0]).abs() < 1e-3 && (y - dst[i][1]).abs() < 1e-3);
        }
    }

    #[test]
    fn invert_round_trips() {
        let m: Affine = [1.2, -0.4, 30.0, 0.4, 1.2, -12.0];
        let inv = invert(&m);
        let (x, y) = apply(&inv, apply(&m, 7.0, 9.0).0, apply(&m, 7.0, 9.0).1);
        assert!((x - 7.0).abs() < 1e-4 && (y - 9.0).abs() < 1e-4);
    }

    #[test]
    fn crop_then_paste_of_the_same_crop_changes_almost_nothing() {
        let (w, h) = (300, 300);
        let mut img = RgbF::new(w, h);
        for (i, p) in img.data.chunks_exact_mut(3).enumerate() {
            let (x, y) = ((i % w) as f32 / w as f32, (i / w) as f32 / h as f32);
            p.copy_from_slice(&[x, y, 0.5 * (x + y)]);
        }
        let reference = img.clone();
        let face = template_face(0.5, 70.0, 60.0); // crop covers 256 px of the image
        let t = crop_transform(&face);
        let crop = extract_crop(&img, &t);
        paste_back(&mut img, &crop, &t);
        let max_diff = img.data.iter().zip(&reference.data).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
        assert!(max_diff < 0.02, "max diff {max_diff}");
    }

    #[test]
    fn paste_only_touches_the_face_area() {
        let mut img = RgbF::new(300, 300);
        let restored = {
            let mut r = RgbF::new(CROP, CROP);
            r.data.iter_mut().for_each(|v| *v = 1.0);
            r
        };
        let t = crop_transform(&template_face(0.5, 70.0, 60.0));
        paste_back(&mut img, &restored, &t);
        let at = |x: usize, y: usize| img.data[(y * 300 + x) * 3];
        assert!(at(70 + 64, 60 + 90) > 0.95, "face center replaced");
        assert_eq!(at(5, 5), 0.0, "far corner untouched");
    }

    #[test]
    fn decodes_a_single_anchor_and_suppresses_duplicates() {
        // 64x64 input: stride 8 -> 8x8 map, 16 -> 4x4, 32 -> 2x2 (two anchors per cell).
        let size = 64;
        let n = |s: usize| (size / s) * (size / s) * 2;
        let mut scores = [vec![0.0f32; n(8)], vec![0.0; n(16)], vec![0.0; n(32)]];
        let boxes = [vec![1.0f32; n(8) * 4], vec![1.0; n(16) * 4], vec![1.0; n(32) * 4]];
        let kps = [vec![0.25f32; n(8) * 10], vec![0.25; n(16) * 10], vec![0.25; n(32) * 10]];
        scores[2][3] = 0.9; // cell 1 -> anchor at x=32, y=0
        scores[2][2] = 0.6; // second anchor of the same cell: identical box, must be suppressed
        let faces = decode_scrfd([&scores[0], &scores[1], &scores[2]], [&boxes[0], &boxes[1], &boxes[2]], [&kps[0], &kps[1], &kps[2]], size, 0.5);
        assert_eq!(faces.len(), 1, "{faces:?}");
        let f = &faces[0];
        assert_eq!(f.bbox, [0.0, -32.0, 64.0, 32.0]);
        assert_eq!(f.landmarks[0], [32.0 + 8.0, 0.0 + 8.0]);
    }

    #[test]
    fn detector_input_is_letterboxed_and_normalized() {
        let mut img = RgbF::new(100, 50);
        img.data.iter_mut().for_each(|v| *v = 1.0);
        let (t, scale) = prepare_detector_input(&img, 64);
        assert!((scale - 0.64).abs() < 1e-6);
        assert!((t[0] - (255.0 - 127.5) / 128.0).abs() < 1e-4, "image area");
        assert!((t[63 * 64 + 63] - (-127.5 / 128.0)).abs() < 1e-4, "padding");
    }

    #[test]
    fn eye_spots_follow_the_landmarks() {
        let f = template_face(1.0, 0.0, 0.0);
        let [l, r] = f.eye_spots();
        assert!(l.0 < r.0 && l.2 > 0.0);
    }

    #[test]
    fn stub_or_real_engine_reports_availability_consistently() {
        assert_eq!(compiled_in(), cfg!(feature = "codeformer"));
    }
}
