//! Shared mutable state of the app.

use pixelforge_core::faces::FaceEngine;
use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Arc, Mutex},
};

#[derive(Default)]
pub struct AppState {
    /// Cancel flags of queued or running jobs, by job id.
    pub cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// Only one job drives the GPU at a time; batches are queued by the UI.
    pub run_lock: Mutex<()>,
    /// Face models, loaded on first use because ONNX Runtime start-up is not free.
    pub faces: Mutex<Option<FaceEngine>>,
    /// Serializes read-modify-write cycles of the history file.
    pub history_lock: Mutex<()>,
}
