//! Where the sidecar, models and DLLs live in an installed app and in a dev checkout.

use pixelforge_core::engine::Engine;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// `src-tauri/` at compile time, used when running from a source checkout.
const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

struct Dirs {
    exe: Vec<PathBuf>,
    models: Vec<PathBuf>,
    dlls: Vec<PathBuf>,
}

fn dirs(app: &AppHandle) -> Dirs {
    let manifest = PathBuf::from(MANIFEST_DIR);
    let mut roots: Vec<PathBuf> = vec![];
    if let Ok(exe) = std::env::current_exe() {
        roots.extend(exe.parent().map(PathBuf::from));
    }
    if let Ok(res) = app.path().resource_dir() {
        roots.push(res.clone());
        roots.push(res.join("resources"));
    }
    roots.push(manifest.join("resources"));
    Dirs {
        exe: roots.iter().cloned().chain([manifest.join("binaries")]).collect(),
        models: roots.iter().map(|r| r.join("models")).collect(),
        dlls: roots.clone(),
    }
}

/// Builds the engine from whatever layout is present.
pub fn engine(app: &AppHandle) -> pixelforge_core::Result<Engine> {
    let d = dirs(app);
    Ok(Engine::locate(&d.exe, &d.models)?.with_dll_dirs(d.dlls))
}

/// Directory holding the face models, if any.
pub fn faces_dir(app: &AppHandle) -> Option<PathBuf> {
    dirs(app).models.into_iter().map(|m| m.join("faces")).find(|d| pixelforge_core::faces::models_present(d))
}

/// Per-user data directory (history).
pub fn data_dir(app: &AppHandle) -> PathBuf {
    app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("pixelforge"))
}

/// Scratch space for intermediate PNGs.
pub fn work_dir() -> PathBuf {
    std::env::temp_dir().join("pixelforge")
}
