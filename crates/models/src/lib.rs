//! Model downloader. Models (STT, LLM) are never bundled: they are fetched on first run into
//! a directory chosen by the caller (the app data dir), with progress, resume and a SHA-256
//! check. Adding a model is a `ModelSpec` (see `presets`), not new code.

mod download;
mod error;
mod presets;
mod spec;

pub use download::{Downloader, Progress, ProgressSink};
pub use error::ModelError;
pub use presets::{all as presets, find as find_preset, whisper_small_q5};
pub use spec::ModelSpec;
pub use tokio_util::sync::CancellationToken;
