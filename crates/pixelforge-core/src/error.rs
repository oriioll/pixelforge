//! Error type shared by the whole pipeline.

use std::path::PathBuf;

/// Everything that can go wrong while enhancing an image.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read or write image: {0}")]
    Image(#[from] image::ImageError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Real-ESRGAN executable not found (looked in: {0})")]
    EngineNotFound(String),

    #[error("model '{0}' is not installed")]
    ModelMissing(String),

    #[error("Real-ESRGAN failed (exit code {code:?}): {stderr}")]
    Sidecar { code: Option<i32>, stderr: String },

    #[error("face restoration failed: {0}")]
    Faces(String),

    #[error("the job was cancelled")]
    Cancelled,

    #[error("unsupported input: {0}")]
    Unsupported(PathBuf),
}

/// Convenience alias used across the crate.
pub type Result<T> = std::result::Result<T, Error>;
