//! The enhancement pipeline: one function turns a job into an output file.
//!
//! Stages: load -> deblock/denoise -> Real-ESRGAN x4 -> supersample -> bokeh protection ->
//! faces -> tone -> selective sharpening -> save.

use crate::buffer::{lerp, RgbF};
use crate::engine::{Engine, MODEL_GENERAL, MODEL_GENERAL_WDN};
use crate::error::{Error, Result};
use crate::faces::{self, Face, FaceEngine};
use crate::filters::resize_bilinear;
use crate::masks::{edge_gate, focus_mask, grass_mask, skin_mask};
use crate::presets::{Params, Preset};
use crate::restore::{restore_source, RestoreParams};
use crate::sharpen::{reinject_detail, spot_plane, unsharp_luma};
use crate::tone::{auto_levels, shadows_highlights, white_balance};
use image::{codecs::jpeg::JpegEncoder, imageops::FilterType, ExtendedColorType, ImageDecoder, ImageEncoder, RgbImage};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::BufWriter,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

/// Output size. 2K and 4K refer to the long edge (2560 / 3840 px), which suits vertical phone photos.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputSize {
    #[serde(rename = "2k")]
    Qhd2k,
    #[serde(rename = "4k")]
    Uhd4k,
    /// Twice the source dimensions.
    #[serde(rename = "x2")]
    Original2x,
}

impl OutputSize {
    /// Output dimensions for a source of `w x h`, keeping the aspect ratio.
    pub fn dims(self, w: u32, h: u32) -> (u32, u32) {
        let target_long = match self {
            OutputSize::Qhd2k => 2560.0,
            OutputSize::Uhd4k => 3840.0,
            OutputSize::Original2x => return (w * 2, h * 2),
        };
        let s = target_long / w.max(h) as f32;
        (((w as f32 * s).round() as u32).max(1), ((h as f32 * s).round() as u32).max(1))
    }

    fn tag(self) -> &'static str {
        match self {
            OutputSize::Qhd2k => "2k",
            OutputSize::Uhd4k => "4k",
            OutputSize::Original2x => "x2",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Png,
    Jpeg,
}

/// Everything the user chose for one image.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnhanceJob {
    pub input: PathBuf,
    pub output: PathBuf,
    pub preset: Preset,
    /// 0..1, 0.6 is the preset's nominal look.
    pub intensity: f32,
    /// ncnn model name, e.g. `realesrgan-x4plus`.
    pub model: String,
    /// 0..1, only used by `realesr-general-x4v3` (1 = strongest denoise).
    pub denoise_strength: f32,
    pub size: OutputSize,
    pub format: OutputFormat,
    pub jpeg_quality: u8,
    pub faces: bool,
    /// Overrides the preset's CodeFormer fidelity.
    pub face_fidelity: Option<f32>,
    /// ncnn tile size, 0 = automatic.
    pub tile: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Loading,
    Restoring,
    Upscaling,
    Faces,
    Tone,
    Sharpening,
    Saving,
    Done,
}

/// Overall progress of a job.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Progress {
    pub stage: Stage,
    /// 0..1 across the whole job.
    pub fraction: f32,
}

/// Resources shared by jobs.
pub struct Context<'a> {
    pub engine: &'a Engine,
    pub faces: Option<&'a mut FaceEngine>,
    pub cancel: &'a AtomicBool,
    /// Scratch directory for the intermediate PNGs.
    pub work_dir: &'a Path,
}

impl Context<'_> {
    fn check(&self) -> Result<()> {
        if self.cancel.load(Ordering::Relaxed) { Err(Error::Cancelled) } else { Ok(()) }
    }
}

/// Runs a job and returns the path written.
///
/// # Errors
/// Any [`Error`]; [`Error::Cancelled`] if the cancel flag was raised between stages.
pub fn run(job: &EnhanceJob, ctx: &mut Context, on_progress: &mut dyn FnMut(Progress)) -> Result<PathBuf> {
    let params = job.preset.resolve(job.intensity);
    let mut emit = |stage: Stage, fraction: f32| on_progress(Progress { stage, fraction: fraction.clamp(0.0, 1.0) });

    emit(Stage::Loading, 0.0);
    let source = load_oriented(&job.input)?;
    let (src_w, src_h) = source.dimensions();
    let (tw, th) = job.size.dims(src_w, src_h);
    let ai_input = fit_for_ai(source, tw.max(th));
    ctx.check()?;

    // 1. Remove codec damage before the upscaler can amplify it.
    emit(Stage::Restoring, 0.04);
    let mut restored = RgbF::from_rgb8(&ai_input);
    restore_source(&mut restored, RestoreParams { deblock: params.deblock, denoise_luma: params.denoise_luma, denoise_chroma: params.denoise_chroma });
    let focus_src = focus_mask(&restored.luma(), restored.w, restored.h);
    let restored8 = restored.to_rgb8();
    ctx.check()?;

    // 2. Real-ESRGAN x4, then supersample (resize down) to the requested size.
    let ai = upscale(job, ctx, &restored8, &mut |f| emit(Stage::Upscaling, 0.10 + 0.58 * f))?;
    ctx.check()?;
    let ai = image::imageops::resize(&ai, tw, th, FilterType::Lanczos3);
    let plain = image::imageops::resize(&restored8, tw, th, FilterType::Lanczos3);
    let mut out = RgbF::from_rgb8(&ai);
    let plain = RgbF::from_rgb8(&plain);
    drop(ai);

    // 3. Bokeh protection: where the source was out of focus, fall back toward a plain resize
    //    instead of keeping texture the network invented.
    let focus = resize_bilinear(&focus_src, restored.w, restored.h, tw as usize, th as usize);
    protect_bokeh(&mut out, &plain, &focus, params.bokeh_floor);

    // 4. Faces.
    emit(Stage::Faces, 0.70);
    let detected = process_faces(job, ctx, &params, &mut out)?;
    ctx.check()?;

    // 5. Skin texture re-injection (waxy-skin countermeasure).
    if params.texture_reinject > 0.0 && !detected.is_empty() {
        let skin = skin_mask(&out);
        let amount: Vec<f32> = skin.iter().map(|s| s * params.texture_reinject).collect();
        reinject_detail(&mut out, &plain, 1.2, &amount);
    }
    drop(plain);

    // 6. Tone.
    emit(Stage::Tone, 0.82);
    white_balance(&mut out, params.white_balance);
    auto_levels(&mut out, params.auto_levels);
    shadows_highlights(&mut out, params.shadows, params.highlights);
    ctx.check()?;

    // 7. Selective sharpening.
    emit(Stage::Sharpening, 0.88);
    sharpen_selectively(&mut out, &focus, &params, &detected);

    emit(Stage::Saving, 0.96);
    save(&out.to_rgb8(), &job.output, job.format, job.jpeg_quality)?;
    emit(Stage::Done, 1.0);
    Ok(job.output.clone())
}

/// Shrinks inputs that would make the 4x output more than twice the target, which only
/// wastes memory and GPU time (the result is supersampled down anyway).
fn fit_for_ai(img: RgbImage, target_long: u32) -> RgbImage {
    let long = img.width().max(img.height());
    let max_long = (target_long * 2 / 4).max(1);
    if long <= max_long {
        return img;
    }
    let s = max_long as f32 / long as f32;
    image::imageops::resize(&img, ((img.width() as f32 * s).round() as u32).max(1), ((img.height() as f32 * s).round() as u32).max(1), FilterType::Lanczos3)
}

/// Runs the chosen model; for `realesr-general-x4v3` with a partial denoise strength it runs
/// the weak-denoise variant too and mixes both outputs (approximation of weight interpolation).
fn upscale(job: &EnhanceJob, ctx: &Context, input: &RgbImage, progress: &mut dyn FnMut(f32)) -> Result<RgbImage> {
    let stamp = format!("pf-{}-{}", std::process::id(), job.output.file_stem().and_then(|s| s.to_str()).unwrap_or("job"));
    std::fs::create_dir_all(ctx.work_dir)?;
    let in_path = ctx.work_dir.join(format!("{stamp}-in.png"));
    input.save(&in_path)?;

    let s = job.denoise_strength.clamp(0.0, 1.0);
    let blend = job.model == MODEL_GENERAL && s < 0.999 && ctx.engine.has_model(MODEL_GENERAL_WDN);
    let runs: Vec<&str> = if blend { vec![MODEL_GENERAL, MODEL_GENERAL_WDN] } else { vec![job.model.as_str()] };
    let mut results = vec![];
    for (i, model) in runs.iter().enumerate() {
        let out_path = ctx.work_dir.join(format!("{stamp}-out{i}.png"));
        let (lo, span) = (i as f32 / runs.len() as f32, 1.0 / runs.len() as f32);
        let r = ctx.engine.upscale(&in_path, &out_path, model, job.tile, &mut |f| progress(lo + span * f), ctx.cancel);
        if let Err(e) = r {
            cleanup(&in_path, &results_paths(ctx.work_dir, &stamp, runs.len()));
            return Err(e);
        }
        results.push(out_path);
    }
    let mut img = image::open(&results[0])?.into_rgb8();
    if blend {
        let weak = image::open(&results[1])?.into_rgb8();
        // s = 1 -> general (strong denoise), s = 0 -> wdn (weak denoise)
        for (a, b) in img.as_mut().iter_mut().zip(weak.as_raw()) {
            *a = (*a as f32 * s + *b as f32 * (1.0 - s)).round() as u8;
        }
    }
    cleanup(&in_path, &results);
    Ok(img)
}

fn results_paths(dir: &Path, stamp: &str, n: usize) -> Vec<PathBuf> {
    (0..n).map(|i| dir.join(format!("{stamp}-out{i}.png"))).collect()
}

fn cleanup(input: &Path, outputs: &[PathBuf]) {
    let _ = std::fs::remove_file(input);
    outputs.iter().for_each(|p| {
        let _ = std::fs::remove_file(p);
    });
}

/// `out = plain + (ai - plain) * w` with `w = floor + (1 - floor) * focus`.
pub fn protect_bokeh(out: &mut RgbF, plain: &RgbF, focus: &[f32], floor: f32) {
    if floor >= 1.0 {
        return;
    }
    for (i, (o, p)) in out.data.chunks_exact_mut(3).zip(plain.data.chunks_exact(3)).enumerate() {
        let w = lerp(floor, 1.0, focus[i]);
        for c in 0..3 {
            o[c] = p[c] + (o[c] - p[c]) * w;
        }
    }
}

/// Detects faces when something needs them and restores them when the user asked for it.
fn process_faces(job: &EnhanceJob, ctx: &mut Context, params: &Params, out: &mut RgbF) -> Result<Vec<Face>> {
    let wants_eyes = params.eye_sharpen > 0.0;
    if !(job.faces || wants_eyes) {
        return Ok(vec![]);
    }
    let Some(engine) = ctx.faces.as_deref_mut() else { return Ok(vec![]) };
    let detected = engine.detect(out)?;
    if job.faces {
        faces::restore_faces(engine, out, &detected, job.face_fidelity.unwrap_or(params.face_fidelity))?;
    }
    Ok(detected)
}

fn sharpen_selectively(out: &mut RgbF, focus: &[f32], p: &Params, detected: &[Face]) {
    let (w, h) = (out.w, out.h);
    let res = w.max(h) as f32 / 2560.0; // keeps the look constant across output sizes
    let gate = edge_gate(&out.luma(), w, h);
    let eg = p.edge_gate;
    let edge_factor = |i: usize| (1.0 - eg) + eg * gate[i];

    let skin = (p.skin_sharpen < 1.0).then(|| skin_mask(out));
    let skin_factor = |i: usize| skin.as_ref().map_or(1.0, |s| lerp(1.0, p.skin_sharpen, s[i]));

    // Main pass: strong on in-focus detail, light on blur.
    let main: Vec<f32> = (0..w * h).map(|i| lerp(p.sharpen_blur, p.sharpen_focus, focus[i]) * edge_factor(i) * skin_factor(i)).collect();
    unsharp_luma(out, 0.8 + 0.7 * res, &main, 0.15);

    // Pitch texture: wider radius so blades read as texture, not as ringing.
    if p.sharpen_grass > 0.0 {
        let grass = grass_mask(out);
        let amount: Vec<f32> = (0..w * h).map(|i| p.sharpen_grass * grass[i] * focus[i].max(0.35) * edge_factor(i)).collect();
        unsharp_luma(out, 2.0 + 1.0 * res, &amount, 0.12);
    }

    // Eyes.
    if p.eye_sharpen > 0.0 && !detected.is_empty() {
        let spots: Vec<(f32, f32, f32)> = detected.iter().flat_map(|f| f.eye_spots()).collect();
        let amount: Vec<f32> = spot_plane(w, h, &spots).iter().map(|s| s * p.eye_sharpen).collect();
        unsharp_luma(out, 1.0 * res.max(0.6), &amount, 0.15);
    }

    // Final pass.
    if p.final_sharpen > 0.0 {
        let amount: Vec<f32> = (0..w * h).map(|i| p.final_sharpen * edge_factor(i)).collect();
        unsharp_luma(out, 0.6 + 0.3 * res, &amount, 0.1);
    }
}

/// Loads an image applying its EXIF orientation.
///
/// # Errors
/// [`Error::Image`] when the file cannot be decoded.
pub fn load_oriented(path: &Path) -> Result<RgbImage> {
    let mut decoder = image::ImageReader::open(path)?.with_guessed_format()?.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut img = image::DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    Ok(img.into_rgb8())
}

/// Writes PNG or JPEG. Metadata is not preserved.
///
/// # Errors
/// I/O or encoding failures.
pub fn save(img: &RgbImage, path: &Path, format: OutputFormat, quality: u8) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = BufWriter::new(File::create(path)?);
    match format {
        OutputFormat::Png => image::codecs::png::PngEncoder::new(file).write_image(img.as_raw(), img.width(), img.height(), ExtendedColorType::Rgb8)?,
        OutputFormat::Jpeg => JpegEncoder::new_with_quality(file, quality.clamp(1, 100)).write_image(img.as_raw(), img.width(), img.height(), ExtendedColorType::Rgb8)?,
    }
    Ok(())
}

/// `<dir>/<name>_pixelforge_<size>.<ext>`, with a numeric suffix if the file already exists.
pub fn default_output_path(input: &Path, out_dir: Option<&Path>, size: OutputSize, format: OutputFormat) -> PathBuf {
    let dir = out_dir.map(Path::to_path_buf).or_else(|| input.parent().map(Path::to_path_buf)).unwrap_or_default();
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
    let ext = match format {
        OutputFormat::Png => "png",
        OutputFormat::Jpeg => "jpg",
    };
    let mut candidate = dir.join(format!("{stem}_pixelforge_{}.{ext}", size.tag()));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem}_pixelforge_{}_{n}.{ext}", size.tag()));
        n += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_dimensions() {
        assert_eq!(OutputSize::Qhd2k.dims(1080, 1920), (1440, 2560));
        assert_eq!(OutputSize::Uhd4k.dims(1920, 1080), (3840, 2160));
        assert_eq!(OutputSize::Original2x.dims(1000, 500), (2000, 1000));
    }

    #[test]
    fn serde_names_match_the_ui() {
        assert_eq!(serde_json::to_string(&OutputSize::Qhd2k).unwrap(), "\"2k\"");
        assert_eq!(serde_json::to_string(&OutputFormat::Jpeg).unwrap(), "\"jpeg\"");
    }

    #[test]
    fn bokeh_floor_blends_toward_the_plain_resize() {
        let mut out = RgbF { w: 2, h: 1, data: vec![1.0; 6] };
        let plain = RgbF { w: 2, h: 1, data: vec![0.0; 6] };
        protect_bokeh(&mut out, &plain, &[0.0, 1.0], 0.25);
        assert!((out.data[0] - 0.25).abs() < 1e-6, "blurred pixel keeps 25% of the AI result");
        assert_eq!(out.data[3], 1.0, "in-focus pixel keeps all of it");
    }

    #[test]
    fn big_inputs_are_shrunk_before_the_upscaler() {
        let big = RgbImage::new(4000, 3000);
        let fit = fit_for_ai(big, 2560);
        assert_eq!(fit.width().max(fit.height()), 1280);
        let small = fit_for_ai(RgbImage::new(1280, 720), 2560);
        assert_eq!(small.width(), 1280, "inputs already within budget are untouched");
    }

    #[test]
    fn output_names_do_not_overwrite() {
        let dir = std::env::temp_dir().join(format!("pf-names-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("foto.jpg");
        let first = default_output_path(&input, None, OutputSize::Qhd2k, OutputFormat::Jpeg);
        assert_eq!(first.file_name().unwrap(), "foto_pixelforge_2k.jpg");
        std::fs::write(&first, b"x").unwrap();
        let second = default_output_path(&input, None, OutputSize::Qhd2k, OutputFormat::Jpeg);
        assert_eq!(second.file_name().unwrap(), "foto_pixelforge_2k_2.jpg");
    }

    #[cfg(unix)]
    mod end_to_end {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        /// Fake Real-ESRGAN that "upscales" by copying a prepared 4x image.
        fn fake_engine(dir: &Path, upscaled: &RgbImage) -> Engine {
            std::fs::create_dir_all(dir.join("models")).unwrap();
            for name in [crate::engine::MODEL_X4PLUS] {
                for ext in ["param", "bin"] {
                    std::fs::write(dir.join("models").join(format!("{name}.{ext}")), b"x").unwrap();
                }
            }
            upscaled.save(dir.join("canned.png")).unwrap();
            let exe = dir.join("realesrgan-ncnn-vulkan");
            std::fs::write(&exe, "#!/bin/sh\nwhile [ $# -gt 0 ]; do case $1 in -o) out=$2; shift;; esac; shift; done\nprintf '50.00%%\\n' >&2\ncp \"$(dirname \"$0\")/canned.png\" \"$out\"\n").unwrap();
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
            Engine { exe, models_dir: dir.join("models") }
        }

        fn job(dir: &Path, preset: Preset) -> EnhanceJob {
            EnhanceJob {
                input: dir.join("in.png"),
                output: dir.join(format!("out_{preset:?}.png")),
                preset,
                intensity: 0.6,
                model: crate::engine::MODEL_X4PLUS.into(),
                denoise_strength: 0.5,
                size: OutputSize::Original2x,
                format: OutputFormat::Png,
                jpeg_quality: 95,
                faces: false,
                face_fidelity: None,
                tile: 0,
            }
        }

        fn run_case(preset: Preset) -> (PathBuf, Vec<Progress>) {
            let dir = std::env::temp_dir().join(format!("pf-e2e-{}-{preset:?}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            // 48x32 source: green pitch bottom, dark top, some texture.
            let src = RgbImage::from_fn(48, 32, |x, y| if y < 12 { image::Rgb([20, 24, 50]) } else { image::Rgb([40 + (x * 3 % 30) as u8, 120, 40]) });
            src.save(dir.join("in.png")).unwrap();
            let canned = image::imageops::resize(&src, 192, 128, FilterType::Triangle);
            let engine = fake_engine(&dir, &canned);
            let cancel = AtomicBool::new(false);
            let mut ctx = Context { engine: &engine, faces: None, cancel: &cancel, work_dir: &dir.join("work") };
            let mut seen = vec![];
            let out = run(&job(&dir, preset), &mut ctx, &mut |p| seen.push(p)).unwrap();
            (out, seen)
        }

        #[test]
        fn both_presets_produce_an_image_of_the_requested_size() {
            for preset in [Preset::Football, Preset::MirrorSelfie] {
                let (out, seen) = run_case(preset);
                let img = image::open(&out).unwrap();
                assert_eq!((img.width(), img.height()), (96, 64), "{preset:?}");
                assert!(seen.windows(2).all(|w| w[1].fraction >= w[0].fraction), "progress never goes back");
                assert_eq!(seen.last().unwrap().stage, Stage::Done);
            }
        }

        #[test]
        fn a_raised_cancel_flag_stops_the_job() {
            let dir = std::env::temp_dir().join(format!("pf-cancel-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            RgbImage::new(16, 16).save(dir.join("in.png")).unwrap();
            let engine = fake_engine(&dir, &RgbImage::new(64, 64));
            let cancel = AtomicBool::new(true);
            let mut ctx = Context { engine: &engine, faces: None, cancel: &cancel, work_dir: &dir.join("work") };
            let err = run(&job(&dir, Preset::Football), &mut ctx, &mut |_| {}).unwrap_err();
            assert!(matches!(err, Error::Cancelled));
        }
    }
}
