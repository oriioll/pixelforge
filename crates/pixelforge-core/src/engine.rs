//! Wrapper around the `realesrgan-ncnn-vulkan` sidecar and its model files.

use crate::error::{Error, Result};
use serde::Serialize;
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    time::Duration,
};

/// ncnn model basenames (a `.param` and a `.bin` file each).
pub const MODEL_X4PLUS: &str = "realesrgan-x4plus";
pub const MODEL_GENERAL: &str = "realesr-general-x4v3";
pub const MODEL_GENERAL_WDN: &str = "realesr-general-wdn-x4v3";

const EXE_STEM: &str = "realesrgan-ncnn-vulkan";

/// A model the UI can offer.
#[derive(Clone, Debug, Serialize)]
pub struct ModelInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub installed: bool,
    /// Whether the denoise-strength slider does anything for this model.
    pub denoise_slider: bool,
}

/// Located sidecar executable plus its models directory.
#[derive(Clone, Debug)]
pub struct Engine {
    pub exe: PathBuf,
    pub models_dir: PathBuf,
}

impl Engine {
    /// Finds the executable and models in the given candidate directories, in order.
    /// `PIXELFORGE_ENGINE` / `PIXELFORGE_MODELS` override the search.
    ///
    /// # Errors
    /// [`Error::EngineNotFound`] if no executable or no models directory is found.
    pub fn locate(exe_dirs: &[PathBuf], model_dirs: &[PathBuf]) -> Result<Self> {
        let exe = std::env::var_os("PIXELFORGE_ENGINE")
            .map(PathBuf::from)
            .or_else(|| exe_dirs.iter().find_map(|d| find_exe(d)))
            .ok_or_else(|| Error::EngineNotFound(join_dirs(exe_dirs)))?;
        let models_dir = std::env::var_os("PIXELFORGE_MODELS")
            .map(PathBuf::from)
            .or_else(|| model_dirs.iter().find(|d| d.join(format!("{MODEL_X4PLUS}.param")).is_file()).cloned())
            .ok_or_else(|| Error::EngineNotFound(join_dirs(model_dirs)))?;
        Ok(Self { exe, models_dir })
    }

    /// True when both ncnn files of `name` exist.
    pub fn has_model(&self, name: &str) -> bool {
        self.models_dir.join(format!("{name}.param")).is_file() && self.models_dir.join(format!("{name}.bin")).is_file()
    }

    /// Models the UI should list, with availability.
    pub fn models(&self) -> Vec<ModelInfo> {
        let general = self.has_model(MODEL_GENERAL);
        vec![
            ModelInfo { id: MODEL_X4PLUS, label: "Real-ESRGAN x4plus (photos)", installed: self.has_model(MODEL_X4PLUS), denoise_slider: false },
            ModelInfo { id: MODEL_GENERAL, label: "Real-ESRGAN general x4v3 (compressed images)", installed: general, denoise_slider: general && self.has_model(MODEL_GENERAL_WDN) },
        ]
    }

    /// Upscales `input` 4x into `output` (PNG). `tile` 0 lets the tool pick a tile size
    /// for the GPU memory available. Progress is reported in 0..1.
    ///
    /// # Errors
    /// [`Error::ModelMissing`], [`Error::Cancelled`] or [`Error::Sidecar`] when the tool exits with an error.
    pub fn upscale(&self, input: &Path, output: &Path, model: &str, tile: u32, progress: &mut dyn FnMut(f32), cancel: &AtomicBool) -> Result<()> {
        if !self.has_model(model) {
            return Err(Error::ModelMissing(model.to_string()));
        }
        let mut cmd = Command::new(&self.exe);
        cmd.args(["-i"]).arg(input).arg("-o").arg(output).args(["-n", model, "-s", "4", "-f", "png", "-t"]).arg(tile.to_string()).arg("-m").arg(&self.models_dir);
        if let Some(dir) = self.exe.parent() {
            cmd.current_dir(dir); // lets Windows find vcomp140.dll next to the exe
        }
        hide_console(&mut cmd);
        let mut child = cmd.stdout(Stdio::null()).stderr(Stdio::piped()).spawn()?;
        let mut stderr = child.stderr.take().expect("stderr is piped");
        let (tx, rx) = mpsc::channel::<String>();
        let reader = std::thread::spawn(move || {
            let mut buf = [0u8; 512];
            while let Ok(n) = stderr.read(&mut buf) {
                if n == 0 || tx.send(String::from_utf8_lossy(&buf[..n]).into_owned()).is_err() {
                    break;
                }
            }
        });

        let mut tail = String::new();
        loop {
            if cancel.load(Ordering::Relaxed) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Cancelled);
            }
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(chunk) => {
                    if let Some(p) = parse_progress(&chunk) {
                        progress((p / 100.0).clamp(0.0, 1.0));
                    }
                    tail.push_str(&chunk);
                    if tail.len() > 2000 {
                        let cut = tail.len() - 2000;
                        let start = tail.char_indices().map(|(i, _)| i).find(|&i| i >= cut).unwrap_or(0);
                        tail = tail[start..].to_string();
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        let status = child.wait()?;
        let _ = reader.join();
        if status.success() {
            Ok(())
        } else {
            Err(Error::Sidecar { code: status.code(), stderr: tail.trim().to_string() })
        }
    }
}

fn join_dirs(dirs: &[PathBuf]) -> String {
    dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>().join(", ")
}

/// Finds an executable named `realesrgan-ncnn-vulkan[...]` in `dir`. The suffixed form
/// (`-x86_64-pc-windows-msvc.exe`) is what Tauri's `externalBin` keeps in the source tree.
fn find_exe(dir: &Path) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            p.is_file() && name.starts_with(EXE_STEM) && (name.ends_with(".exe") || !name.contains('.'))
        })
        .collect();
    found.sort_by_key(|p| p.as_os_str().len()); // prefer the un-suffixed name
    found.into_iter().next()
}

/// Extracts the last `NN.NN%` value from a chunk of the tool's stderr.
pub fn parse_progress(chunk: &str) -> Option<f32> {
    chunk
        .split(|c: char| c.is_whitespace())
        .filter_map(|t| t.strip_suffix('%').and_then(|n| n.parse::<f32>().ok()))
        .last()
}

#[cfg(windows)]
fn hide_console(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
}

#[cfg(not(windows))]
fn hide_console(_: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_progress_lines() {
        assert_eq!(parse_progress("0.00%\n12.50%\r"), Some(12.5));
        assert_eq!(parse_progress("[0 NVIDIA GeForce]  queueC=2\n"), None);
        assert_eq!(parse_progress("100.00%"), Some(100.0));
    }

    #[cfg(unix)]
    mod fake_sidecar {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        /// Creates a directory with a fake `realesrgan-ncnn-vulkan` that copies input to output.
        fn setup(script_body: &str) -> (PathBuf, Engine) {
            let dir = std::env::temp_dir().join(format!("pf-engine-{}-{:?}", std::process::id(), std::thread::current().id()).replace(['(', ')', ' '], ""));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("models")).unwrap();
            for ext in ["param", "bin"] {
                std::fs::write(dir.join("models").join(format!("{MODEL_X4PLUS}.{ext}")), b"x").unwrap();
            }
            let exe = dir.join(EXE_STEM);
            std::fs::write(&exe, format!("#!/bin/sh\nwhile [ $# -gt 0 ]; do case $1 in -i) in=$2; shift;; -o) out=$2; shift;; esac; shift; done\n{script_body}\n")).unwrap();
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
            let engine = Engine::locate(&[dir.clone()], &[dir.join("models")]).unwrap();
            (dir, engine)
        }

        #[test]
        fn runs_the_tool_and_reports_progress() {
            let (dir, engine) = setup("printf '10.00%%\\n' >&2; sleep 0.2; printf '80.00%%\\n' >&2; cp \"$in\" \"$out\"");
            let input = dir.join("in.png");
            std::fs::write(&input, b"png").unwrap();
            let mut seen = vec![];
            engine.upscale(&input, &dir.join("out.png"), MODEL_X4PLUS, 0, &mut |p| seen.push(p), &AtomicBool::new(false)).unwrap();
            assert!(dir.join("out.png").is_file());
            assert!(seen.iter().any(|p| (*p - 0.8).abs() < 1e-3), "progress seen: {seen:?}");
        }

        #[test]
        fn surfaces_stderr_when_the_tool_fails() {
            let (dir, engine) = setup("echo 'invalid gpu device' >&2; exit 3");
            let err = engine.upscale(&dir.join("a.png"), &dir.join("b.png"), MODEL_X4PLUS, 0, &mut |_| {}, &AtomicBool::new(false)).unwrap_err();
            assert!(matches!(&err, Error::Sidecar { code: Some(3), stderr } if stderr.contains("invalid gpu")), "{err}");
        }

        #[test]
        fn can_be_cancelled() {
            let (dir, engine) = setup("sleep 10");
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let c2 = cancel.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                c2.store(true, Ordering::Relaxed);
            });
            let started = std::time::Instant::now();
            let err = engine.upscale(&dir.join("a.png"), &dir.join("b.png"), MODEL_X4PLUS, 0, &mut |_| {}, &cancel).unwrap_err();
            assert!(matches!(err, Error::Cancelled));
            assert!(started.elapsed() < Duration::from_secs(5));
        }

        #[test]
        fn missing_model_is_reported_before_spawning() {
            let (dir, engine) = setup("exit 0");
            let err = engine.upscale(&dir.join("a.png"), &dir.join("b.png"), "nope", 0, &mut |_| {}, &AtomicBool::new(false)).unwrap_err();
            assert!(matches!(err, Error::ModelMissing(_)));
        }
    }
}
