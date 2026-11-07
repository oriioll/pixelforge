//! Commands invoked from the UI.

use crate::{history, paths, state::AppState};
use pixelforge_core::{
    detect::{self, Suggestion},
    engine::ModelInfo,
    faces::{self, FaceEngine},
    pipeline::{self, Context, EnhanceJob, OutputFormat, OutputSize, Progress},
    presets::Preset,
    Error,
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{atomic::{AtomicBool, Ordering}, Arc},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager};

/// Event carrying [`ProgressEvent`] payloads.
const PROGRESS_EVENT: &str = "pixelforge:progress";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvInfo {
    pub engine_found: bool,
    pub engine_error: Option<String>,
    pub models: Vec<ModelInfo>,
    /// CodeFormer is compiled in and its model files are installed.
    pub faces_available: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInfo {
    pub path: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub suggestion: Suggestion,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    id: String,
    #[serde(flatten)]
    progress: Progress,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnhanceRequest {
    pub id: String,
    pub input: PathBuf,
    pub out_dir: Option<PathBuf>,
    pub preset: Preset,
    pub intensity: f32,
    pub model: String,
    pub denoise_strength: f32,
    pub size: OutputSize,
    pub format: OutputFormat,
    pub jpeg_quality: u8,
    pub faces: bool,
    pub face_fidelity: Option<f32>,
    pub tile: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnhanceResult {
    pub id: String,
    pub output: String,
    pub width: u32,
    pub height: u32,
    pub duration_ms: u64,
    /// Machine-readable notes, e.g. `faces_unavailable`.
    pub warnings: Vec<String>,
}

fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> impl std::future::Future<Output = Result<T, String>> {
    async move { tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())? }
}

/// Reports what is installed so the UI can disable missing options.
#[tauri::command]
pub fn environment(app: AppHandle) -> EnvInfo {
    let faces_available = faces::compiled_in() && paths::faces_dir(&app).is_some();
    match paths::engine(&app) {
        Ok(engine) => EnvInfo { engine_found: true, engine_error: None, models: engine.models(), faces_available },
        Err(e) => EnvInfo { engine_found: false, engine_error: Some(e.to_string()), models: vec![], faces_available },
    }
}

/// Reads an image's size, suggests a preset and lets the webview display the file.
#[tauri::command]
pub async fn inspect_image(app: AppHandle, path: String) -> Result<ImageInfo, String> {
    let p = PathBuf::from(&path);
    app.asset_protocol_scope().allow_file(&p).map_err(|e| e.to_string())?;
    blocking(move || {
        let img = pipeline::load_oriented(&p).map_err(|e| e.to_string())?;
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Ok(ImageInfo { path, name, width: img.width(), height: img.height(), suggestion: detect::suggest(&img) })
    })
    .await
}

/// Runs one job, emitting `pixelforge:progress` events.
#[tauri::command]
pub async fn enhance(app: AppHandle, req: EnhanceRequest) -> Result<EnhanceResult, String> {
    let cancel = Arc::new(AtomicBool::new(false));
    app.state::<AppState>().cancels.lock().unwrap().insert(req.id.clone(), cancel.clone());
    let id = req.id.clone();
    let handle = app.clone();
    let result = blocking(move || enhance_blocking(handle, req, cancel)).await;
    app.state::<AppState>().cancels.lock().unwrap().remove(&id);
    if let Ok(done) = &result {
        let _ = app.asset_protocol_scope().allow_file(&done.output);
    }
    result
}

fn enhance_blocking(app: AppHandle, req: EnhanceRequest, cancel: Arc<AtomicBool>) -> Result<EnhanceResult, String> {
    let state = app.state::<AppState>();
    let _gpu = state.run_lock.lock().unwrap();
    let started = Instant::now();
    let engine = paths::engine(&app).map_err(|e| e.to_string())?;
    let params = req.preset.resolve(req.intensity);
    let mut warnings = vec![];

    // Face models: load once, keep for the next job.
    let mut faces_guard = state.faces.lock().unwrap();
    if (req.faces || params.eye_sharpen > 0.0) && faces_guard.is_none() {
        match paths::faces_dir(&app).filter(|_| faces::compiled_in()).map(|d| FaceEngine::load(&d)) {
            Some(Ok(engine)) => *faces_guard = Some(engine),
            Some(Err(e)) => warnings.push(format!("faces_failed: {e}")),
            None => {}
        }
    }
    if req.faces && faces_guard.is_none() {
        warnings.push("faces_unavailable".into());
    }

    let job = EnhanceJob {
        output: pipeline::default_output_path(&req.input, req.out_dir.as_deref(), req.size, req.format),
        input: req.input,
        preset: req.preset,
        intensity: req.intensity,
        model: req.model.clone(),
        denoise_strength: req.denoise_strength,
        size: req.size,
        format: req.format,
        jpeg_quality: req.jpeg_quality,
        faces: req.faces,
        face_fidelity: req.face_fidelity,
        tile: req.tile,
    };
    let work = paths::work_dir();
    let mut ctx = Context { engine: &engine, faces: faces_guard.as_mut(), cancel: &cancel, work_dir: &work };
    let id = req.id.clone();
    let outcome = pipeline::run(&job, &mut ctx, &mut |progress| {
        let _ = app.emit(PROGRESS_EVENT, ProgressEvent { id: id.clone(), progress });
    })
    .map_err(|e| match e {
        Error::Cancelled => "cancelled".to_string(),
        other => other.to_string(),
    })?;

    let duration_ms = started.elapsed().as_millis() as u64;
    let _hist = state.history_lock.lock().unwrap();
    let _ = history::push(
        &paths::data_dir(&app),
        history::HistoryEntry {
            id: req.id.clone(),
            input: job.input.to_string_lossy().into_owned(),
            output: outcome.path.to_string_lossy().into_owned(),
            preset: serde_json::to_value(job.preset).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default(),
            intensity: job.intensity,
            size: serde_json::to_value(job.size).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default(),
            model: req.model,
            width: outcome.width,
            height: outcome.height,
            created_at: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
            duration_ms,
        },
    );
    Ok(EnhanceResult { id: req.id, output: outcome.path.to_string_lossy().into_owned(), width: outcome.width, height: outcome.height, duration_ms, warnings })
}

/// Raises the cancel flag of a queued or running job.
#[tauri::command]
pub fn cancel_job(app: AppHandle, id: String) {
    if let Some(flag) = app.state::<AppState>().cancels.lock().unwrap().get(&id) {
        flag.store(true, Ordering::Relaxed);
    }
}

/// Lets the webview display a past result.
#[tauri::command]
pub fn history_list(app: AppHandle) -> Vec<history::HistoryEntry> {
    let entries = history::load(&paths::data_dir(&app));
    for e in &entries {
        let _ = app.asset_protocol_scope().allow_file(&e.input);
        let _ = app.asset_protocol_scope().allow_file(&e.output);
    }
    entries
}

#[tauri::command]
pub fn history_remove(app: AppHandle, id: String) -> Result<(), String> {
    let _guard = app.state::<AppState>().history_lock.lock().unwrap();
    let dir = paths::data_dir(&app);
    let kept: Vec<_> = history::load(&dir).into_iter().filter(|e| e.id != id).collect();
    history::save(&dir, &kept).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn history_clear(app: AppHandle) -> Result<(), String> {
    let _guard = app.state::<AppState>().history_lock.lock().unwrap();
    history::save(&paths::data_dir(&app), &[]).map_err(|e| e.to_string())
}
