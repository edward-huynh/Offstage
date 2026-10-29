use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("server answered HTTP {0}")]
    Status(u16),
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("download ended early: got {got} of {expected} bytes (partial file kept for resume)")]
    Incomplete { got: u64, expected: u64 },
    #[error("download is larger than expected ({expected} bytes)")]
    TooLarge { expected: u64 },
    #[error("download cancelled")]
    Cancelled,
}

impl ModelError {
    pub(crate) fn io(path: &std::path::Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.to_path_buf(),
            source,
        }
    }
}
