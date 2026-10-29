use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use offstage_models::{CancellationToken, Downloader, ModelError, ModelSpec, Progress};
use sha2::{Digest, Sha256};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

fn body() -> Vec<u8> {
    (0..2 * 1024 * 1024 + 12_345)
        .map(|i: usize| (i * 31 % 251) as u8)
        .collect()
}

fn spec_for(server: &MockServer, data: &[u8]) -> ModelSpec {
    ModelSpec {
        id: "test-model".into(),
        file_name: "test-model.bin".into(),
        url: format!("{}/model.bin", server.uri()),
        sha256: format!("{:x}", Sha256::digest(data)),
        size: data.len() as u64,
        license: "MIT".into(),
        license_url: None,
    }
}

/// Serves `data` like a real file server: honours `Range: bytes=N-` when `honor_range` is set.
struct FileServer {
    data: Vec<u8>,
    honor_range: bool,
    ranges: Arc<Mutex<Vec<Option<String>>>>,
}

impl Respond for FileServer {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let range = request
            .headers
            .get("range")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        self.ranges.lock().unwrap().push(range.clone());

        let start = range
            .as_deref()
            .and_then(|r| r.strip_prefix("bytes="))
            .and_then(|r| r.strip_suffix('-'))
            .and_then(|r| r.parse::<usize>().ok());
        match start {
            Some(start) if self.honor_range => {
                if start >= self.data.len() {
                    return ResponseTemplate::new(416);
                }
                let last = self.data.len() - 1;
                ResponseTemplate::new(206)
                    .insert_header(
                        "content-range",
                        format!("bytes {start}-{last}/{}", self.data.len()),
                    )
                    .set_body_bytes(self.data[start..].to_vec())
            }
            _ => ResponseTemplate::new(200).set_body_bytes(self.data.clone()),
        }
    }
}

type Ranges = Arc<Mutex<Vec<Option<String>>>>;

async fn serve(data: &[u8], honor_range: bool, expected_requests: u64) -> (MockServer, Ranges) {
    let server = MockServer::start().await;
    let ranges: Ranges = Arc::default();
    Mock::given(method("GET"))
        .and(path("/model.bin"))
        .respond_with(FileServer {
            data: data.to_vec(),
            honor_range,
            ranges: ranges.clone(),
        })
        .expect(expected_requests)
        .mount(&server)
        .await;
    (server, ranges)
}

fn recorder() -> (Arc<Mutex<Vec<Progress>>>, impl Fn(Progress) + Send + Sync) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink_events = events.clone();
    (events, move |p| sink_events.lock().unwrap().push(p))
}

fn assert_monotonic(events: &[Progress], total: u64) {
    assert!(!events.is_empty());
    assert!(events
        .windows(2)
        .all(|w| w[0].downloaded <= w[1].downloaded));
    assert!(events.iter().all(|e| e.total == total));
    assert_eq!(events.last().map(|e| e.downloaded), Some(total));
}

fn no_leftovers(dir: &Path, spec: &ModelSpec) {
    assert!(!spec.part_path_in(dir).exists(), ".part file left behind");
}

#[tokio::test]
async fn downloads_full_file_with_monotonic_progress() {
    let data = body();
    let (server, _) = serve(&data, true, 1).await;
    let spec = spec_for(&server, &data);
    let dir = tempfile::tempdir().unwrap();
    let (events, sink) = recorder();

    let path = Downloader::new()
        .unwrap()
        .ensure(&spec, dir.path(), &sink, &CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(path, spec.path_in(dir.path()));
    assert_eq!(std::fs::read(&path).unwrap(), data);
    no_leftovers(dir.path(), &spec);
    let events = events.lock().unwrap();
    assert!(events.len() > 2, "expected several progress events");
    assert_monotonic(&events, spec.size);
}

#[tokio::test]
async fn resumes_from_part_file_with_range_request() {
    let data = body();
    let (server, ranges) = serve(&data, true, 1).await;
    let spec = spec_for(&server, &data);
    let dir = tempfile::tempdir().unwrap();
    let have = 700_000;
    std::fs::write(spec.part_path_in(dir.path()), &data[..have]).unwrap();
    let (events, sink) = recorder();

    let path = Downloader::new()
        .unwrap()
        .ensure(&spec, dir.path(), &sink, &CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), data);
    no_leftovers(dir.path(), &spec);
    assert_eq!(
        *ranges.lock().unwrap(),
        vec![Some(format!("bytes={have}-"))]
    );
    let events = events.lock().unwrap();
    assert_eq!(
        events[0].downloaded, have as u64,
        "progress starts at the resume point"
    );
    assert_monotonic(&events, spec.size);
}

#[tokio::test]
async fn restarts_when_server_ignores_range() {
    let data = body();
    let (server, ranges) = serve(&data, false, 1).await;
    let spec = spec_for(&server, &data);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(spec.part_path_in(dir.path()), &data[..500_000]).unwrap();

    let path = Downloader::new()
        .unwrap()
        .ensure(&spec, dir.path(), &|_| {}, &CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), data);
    assert_eq!(ranges.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn bad_part_file_is_discarded_and_download_restarts() {
    let data = body();
    // First request (with Range) gets 416 for a part file that is not a prefix of the model
    // but has a valid length; the second request has no Range and gets the full file.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/model.bin"))
        .respond_with(|request: &Request| {
            if request.headers.contains_key("range") {
                ResponseTemplate::new(416)
            } else {
                ResponseTemplate::new(200).set_body_bytes(body())
            }
        })
        .expect(2)
        .mount(&server)
        .await;
    let spec = spec_for(&server, &data);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(spec.part_path_in(dir.path()), vec![0u8; 1000]).unwrap();

    let path = Downloader::new()
        .unwrap()
        .ensure(&spec, dir.path(), &|_| {}, &CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), data);
}

#[tokio::test]
async fn oversized_part_file_is_discarded() {
    let data = body();
    let (server, ranges) = serve(&data, true, 1).await;
    let spec = spec_for(&server, &data);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(spec.part_path_in(dir.path()), vec![1u8; data.len() + 10]).unwrap();

    let path = Downloader::new()
        .unwrap()
        .ensure(&spec, dir.path(), &|_| {}, &CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), data);
    assert_eq!(*ranges.lock().unwrap(), vec![None]);
}

#[tokio::test]
async fn wrong_checksum_deletes_file_and_errors() {
    let data = body();
    let (server, _) = serve(&data, true, 1).await;
    let mut spec = spec_for(&server, &data);
    spec.sha256 = "0".repeat(64);
    let dir = tempfile::tempdir().unwrap();

    let err = Downloader::new()
        .unwrap()
        .ensure(&spec, dir.path(), &|_| {}, &CancellationToken::new())
        .await
        .unwrap_err();

    assert!(matches!(err, ModelError::ChecksumMismatch { .. }), "{err}");
    assert!(!spec.path_in(dir.path()).exists());
    no_leftovers(dir.path(), &spec);
}

#[tokio::test]
async fn skips_download_when_valid_file_exists() {
    let data = body();
    let (server, _) = serve(&data, true, 0).await;
    let spec = spec_for(&server, &data);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(spec.path_in(dir.path()), &data).unwrap();
    let (events, sink) = recorder();

    let path = Downloader::new()
        .unwrap()
        .ensure(&spec, dir.path(), &sink, &CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(path, spec.path_in(dir.path()));
    assert_monotonic(&events.lock().unwrap(), spec.size);
}

#[tokio::test]
async fn redownloads_when_existing_file_is_corrupt() {
    let data = body();
    let (server, _) = serve(&data, true, 1).await;
    let spec = spec_for(&server, &data);
    let dir = tempfile::tempdir().unwrap();
    let mut corrupt = data.clone();
    corrupt[1000] ^= 0xff;
    std::fs::write(spec.path_in(dir.path()), corrupt).unwrap();

    let path = Downloader::new()
        .unwrap()
        .ensure(&spec, dir.path(), &|_| {}, &CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), data);
}

#[tokio::test]
async fn cancel_keeps_part_file_and_next_call_resumes() {
    let data = body();
    let (server, ranges) = serve(&data, true, 2).await;
    let spec = spec_for(&server, &data);
    let dir = tempfile::tempdir().unwrap();
    let downloader = Downloader::new().unwrap();

    let cancel = CancellationToken::new();
    let seen = Arc::new(AtomicU64::new(0));
    let sink = {
        let (cancel, seen) = (cancel.clone(), seen.clone());
        move |p: Progress| {
            seen.store(p.downloaded, Ordering::SeqCst);
            if p.downloaded > 0 {
                cancel.cancel();
            }
        }
    };
    let err = downloader
        .ensure(&spec, dir.path(), &sink, &cancel)
        .await
        .unwrap_err();
    assert!(matches!(err, ModelError::Cancelled), "{err}");
    assert!(!spec.path_in(dir.path()).exists());
    let kept = std::fs::metadata(spec.part_path_in(dir.path()))
        .unwrap()
        .len();
    assert!(kept > 0 && kept < spec.size);

    let path = downloader
        .ensure(&spec, dir.path(), &|_| {}, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), data);
    assert_eq!(
        ranges.lock().unwrap().last().cloned().flatten(),
        Some(format!("bytes={kept}-"))
    );
}

#[tokio::test]
async fn already_cancelled_token_does_nothing() {
    let data = body();
    let (server, _) = serve(&data, true, 0).await;
    let spec = spec_for(&server, &data);
    let dir = tempfile::tempdir().unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();

    let err = Downloader::new()
        .unwrap()
        .ensure(&spec, dir.path(), &|_| {}, &cancel)
        .await
        .unwrap_err();
    assert!(matches!(err, ModelError::Cancelled));
}

#[tokio::test]
async fn http_error_status_is_reported() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let spec = spec_for(&server, b"x");
    let dir = tempfile::tempdir().unwrap();

    let err = Downloader::new()
        .unwrap()
        .ensure(&spec, dir.path(), &|_| {}, &CancellationToken::new())
        .await
        .unwrap_err();
    assert!(matches!(err, ModelError::Status(404)), "{err}");
    assert!(!spec.path_in(dir.path()).exists());
}

#[tokio::test]
async fn truncated_download_is_incomplete_and_kept_for_resume() {
    let data = body();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(data[..1000].to_vec()))
        .mount(&server)
        .await;
    let spec = spec_for(&server, &data);
    let dir = tempfile::tempdir().unwrap();

    let err = Downloader::new()
        .unwrap()
        .ensure(&spec, dir.path(), &|_| {}, &CancellationToken::new())
        .await
        .unwrap_err();
    assert!(
        matches!(err, ModelError::Incomplete { got: 1000, .. }),
        "{err}"
    );
    assert_eq!(
        std::fs::metadata(spec.part_path_in(dir.path()))
            .unwrap()
            .len(),
        1000
    );
}
