//! pixelforge-core: the image enhancement pipeline behind the pixelforge desktop app.
//!
//! The crate is UI-agnostic. The Tauri shell only turns an [`pipeline::EnhanceJob`]
//! into progress events and files on disk.

pub mod buffer;
pub mod engine;
pub mod error;
pub mod faces;
pub mod filters;
pub mod masks;
pub mod presets;
pub mod restore;
pub mod sharpen;
pub mod tone;

pub use error::{Error, Result};
