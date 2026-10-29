use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::StatusCode;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::{ModelError, ModelSpec};

/// Minimum number of new bytes between two progress events (the last one is always sent).
const EMIT_EVERY: u64 = 128 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub downloaded: u64,
    pub total: u64,
}

/// Receives download progress. Any `Fn(Progress)` closure works; the app wires it to `AppEvent`.
pub trait ProgressSink: Send + Sync {
    fn on_progress(&self, progress: Progress);
}

impl<F> ProgressSink for F
where
    F: Fn(Progress) + Send + Sync,
{
    fn on_progress(&self, progress: Progress) {
        self(progress)
    }
}

pub struct Downloader {
    client: reqwest::Client,
}

impl Downloader {
    pub fn new() -> Result<Self, ModelError> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("offstage-models/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            // A stalled connection fails instead of hanging; the `.part` file allows resuming.
            .read_timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self { client })
    }

    /// Makes sure `spec` exists, complete and verified, in `dir`, and returns its path.
    ///
    /// - A final file with the right checksum is reused without any network call.
    /// - A leftover `<file>.part` is resumed with an HTTP Range request.
    /// - On checksum mismatch the partial file is deleted and an error is returned.
    /// - Cancelling keeps the `.part` file so the next call resumes.
    pub async fn ensure(
        &self,
        spec: &ModelSpec,
        dir: &Path,
        sink: &dyn ProgressSink,
        cancel: &CancellationToken,
    ) -> Result<PathBuf, ModelError> {
        if cancel.is_cancelled() {
            return Err(ModelError::Cancelled);
        }
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|e| ModelError::io(dir, e))?;

        let dest = spec.path_in(dir);
        if is_valid(&dest, spec).await? {
            sink.on_progress(Progress {
                downloaded: spec.size,
                total: spec.size,
            });
            return Ok(dest);
        }

        let part = spec.part_path_in(dir);
        self.download_to_part(spec, &part, sink, cancel).await?;

        let actual = sha256_of(&part).await?;
        if !actual.eq_ignore_ascii_case(&spec.sha256) {
            remove_if_exists(&part).await?;
            return Err(ModelError::ChecksumMismatch {
                expected: spec.sha256.to_lowercase(),
                actual,
            });
        }
        tokio::fs::rename(&part, &dest)
            .await
            .map_err(|e| ModelError::io(&dest, e))?;
        Ok(dest)
    }

    async fn download_to_part(
        &self,
        spec: &ModelSpec,
        part: &Path,
        sink: &dyn ProgressSink,
        cancel: &CancellationToken,
    ) -> Result<(), ModelError> {
        let mut offset = match tokio::fs::metadata(part).await {
            Ok(m) => m.len(),
            Err(e) if e.kind() == ErrorKind::NotFound => 0,
            Err(e) => return Err(ModelError::io(part, e)),
        };
        if offset > spec.size {
            remove_if_exists(part).await?;
            offset = 0;
        }
        if offset == spec.size {
            // Complete but never verified/renamed (e.g. interrupted right after the last byte).
            sink.on_progress(Progress {
                downloaded: offset,
                total: spec.size,
            });
            return Ok(());
        }

        loop {
            let mut request = self.client.get(&spec.url);
            if offset > 0 {
                request = request.header(RANGE, format!("bytes={offset}-"));
            }
            let response = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(ModelError::Cancelled),
                r = request.send() => r?,
            };

            let append = match response.status() {
                StatusCode::PARTIAL_CONTENT if range_start(&response) == Some(offset) => true,
                // Server ignored the Range header and sends the whole file: start over.
                StatusCode::OK => false,
                // Stale or unusable partial file: drop it and ask for the whole file.
                StatusCode::RANGE_NOT_SATISFIABLE | StatusCode::PARTIAL_CONTENT if offset > 0 => {
                    remove_if_exists(part).await?;
                    offset = 0;
                    continue;
                }
                status => return Err(ModelError::Status(status.as_u16())),
            };

            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(append)
                .truncate(!append)
                .open(part)
                .await
                .map_err(|e| ModelError::io(part, e))?;
            let start = if append { offset } else { 0 };

            let streamed =
                stream_body(response, &mut file, part, start, spec.size, sink, cancel).await;
            let flushed = file.flush().await.map_err(|e| ModelError::io(part, e));
            drop(file);
            if let Err(ModelError::TooLarge { .. }) = streamed {
                remove_if_exists(part).await?;
            }
            streamed?;
            return flushed;
        }
    }
}

async fn stream_body(
    mut response: reqwest::Response,
    file: &mut tokio::fs::File,
    part: &Path,
    start: u64,
    total: u64,
    sink: &dyn ProgressSink,
    cancel: &CancellationToken,
) -> Result<(), ModelError> {
    let mut downloaded = start;
    let mut last_emit = start;
    sink.on_progress(Progress { downloaded, total });

    loop {
        let chunk = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(ModelError::Cancelled),
            c = response.chunk() => c?,
        };
        let Some(chunk) = chunk else { break };
        file.write_all(&chunk)
            .await
            .map_err(|e| ModelError::io(part, e))?;
        downloaded += chunk.len() as u64;
        if downloaded > total {
            return Err(ModelError::TooLarge { expected: total });
        }
        if downloaded - last_emit >= EMIT_EVERY {
            last_emit = downloaded;
            sink.on_progress(Progress { downloaded, total });
        }
    }

    if downloaded < total {
        return Err(ModelError::Incomplete {
            got: downloaded,
            expected: total,
        });
    }
    if last_emit != downloaded {
        sink.on_progress(Progress { downloaded, total });
    }
    Ok(())
}

/// Start offset of a `Content-Range: bytes <start>-<end>/<total>` header.
fn range_start(response: &reqwest::Response) -> Option<u64> {
    let value = response.headers().get(CONTENT_RANGE)?.to_str().ok()?;
    let range = value.strip_prefix("bytes ")?;
    range.split('-').next()?.parse().ok()
}

/// True when `path` exists with the expected size and SHA-256.
async fn is_valid(path: &Path, spec: &ModelSpec) -> Result<bool, ModelError> {
    match tokio::fs::metadata(path).await {
        Ok(m) if m.len() == spec.size => {
            Ok(sha256_of(path).await?.eq_ignore_ascii_case(&spec.sha256))
        }
        Ok(_) => Ok(false),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
        Err(e) => Err(ModelError::io(path, e)),
    }
}

async fn sha256_of(path: &Path) -> Result<String, ModelError> {
    let owned = path.to_path_buf();
    let result = tokio::task::spawn_blocking(move || -> std::io::Result<String> {
        let mut file = std::fs::File::open(owned)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    })
    .await
    .map_err(|e| ModelError::io(path, std::io::Error::other(e)))?;
    result.map_err(|e| ModelError::io(path, e))
}

async fn remove_if_exists(path: &Path) -> Result<(), ModelError> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(ModelError::io(path, e)),
    }
}
