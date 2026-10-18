//! Download tests against a local server that serves a small fake archive:
//! success, hash mismatch, cancel, offline then restored, and a connection
//! dropped mid-file (resume gives a correct hash). Unix only: the fake
//! browser is a shell script.

#![cfg(unix)]

use std::io::Write as _;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::*;
use crate::types::BrowserSource;

const PLATFORM: &str = "linux64";

/// A zip shaped like Chrome for Testing's: one folder holding the browser
/// (a script printing its version) and a large stored file, so the body
/// arrives in several chunks.
fn fake_archive() -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut out);
        let exe = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
        zip.start_file(
            format!("chrome-headless-shell-{PLATFORM}/chrome-headless-shell"),
            exe,
        )
        .unwrap();
        zip.write_all(b"#!/bin/sh\necho 'Chrome Headless Shell 155.0.8059.39'\n")
            .unwrap();
        let stored =
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file(format!("chrome-headless-shell-{PLATFORM}/blob.bin"), stored)
            .unwrap();
        let blob: Vec<u8> = (0..300_000u32).map(|i| (i.wrapping_mul(31) >> 3) as u8).collect();
        zip.write_all(&blob).unwrap();
        zip.finish().unwrap();
    }
    out.into_inner()
}

fn sha(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn plan(addr: SocketAddr, body: &[u8], sha256: &'static str) -> DownloadPlan {
    DownloadPlan {
        version: "155.0.8059.39",
        platform: PLATFORM,
        url: format!("http://{addr}/browser.zip"),
        bytes: body.len() as u64,
        sha256,
    }
}

fn fast() -> DownloadOptions {
    DownloadOptions {
        retry_every: Duration::from_millis(50),
        give_up_after: Some(Duration::from_secs(20)),
    }
}

struct Fixture {
    ranges: Arc<Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Serves `body` for GET (honouring `Range`) and HEAD. With `cut_first_at`,
/// the first full GET sends that many bytes and closes the connection.
fn serve(listener: TcpListener, body: Vec<u8>, cut_first_at: Option<usize>) -> Fixture {
    let ranges = Arc::new(Mutex::new(Vec::new()));
    let seen = ranges.clone();
    let task = tokio::spawn(async move {
        let mut cut = cut_first_at;
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let mut buf = vec![0u8; 4096];
            let mut n = 0;
            loop {
                let k = sock.read(&mut buf[n..]).await.unwrap_or(0);
                if k == 0 {
                    break;
                }
                n += k;
                if buf[..n].windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let head = String::from_utf8_lossy(&buf[..n]).to_string();
            let range_line = head
                .lines()
                .find(|l| l.to_ascii_lowercase().starts_with("range:"))
                .map(str::to_owned);
            let start = range_line
                .as_deref()
                .and_then(|l| l.split("bytes=").nth(1))
                .and_then(|v| v.trim().trim_end_matches('-').parse::<usize>().ok());
            if let Some(l) = &range_line {
                seen.lock()
                    .unwrap()
                    .push(l.split("bytes=").nth(1).unwrap_or("").trim().to_owned());
            }
            let total = body.len();
            if head.starts_with("HEAD") {
                let h = format!("HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nConnection: close\r\n\r\n");
                let _ = sock.write_all(h.as_bytes()).await;
            } else if let Some(s) = start {
                let h = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {s}-{}/{total}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    total - 1,
                    total - s
                );
                let _ = sock.write_all(h.as_bytes()).await;
                let _ = sock.write_all(&body[s..]).await;
            } else {
                let h = format!("HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nConnection: close\r\n\r\n");
                let _ = sock.write_all(h.as_bytes()).await;
                match cut.take() {
                    Some(at) => {
                        let _ = sock.write_all(&body[..at]).await;
                        let _ = sock.flush().await;
                    }
                    None => {
                        let _ = sock.write_all(&body).await;
                    }
                }
            }
            let _ = sock.shutdown().await;
        }
    });
    Fixture { ranges, task }
}

async fn bind() -> (TcpListener, SocketAddr) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let a = l.local_addr().unwrap();
    (l, a)
}

fn no_progress() -> impl Fn(DownloadProgress) + Send + Sync {
    |_| {}
}

#[test]
fn the_pins_are_complete_and_well_formed() {
    assert_eq!(pins::VERSION, crate::PINNED_VERSION);
    for pin in pins::PINS {
        assert!(valid_sha256(pin.sha256), "{}", pin.platform);
        assert!(pin.bytes > 1_000_000);
        assert!(pins::archive_url(pin.platform).starts_with(&format!(
            "https://{}/chrome-for-testing-public/{}/{}/",
            pins::HOST,
            pins::VERSION,
            pin.platform
        )));
    }
    assert!(pins::pin_for("win32").is_none());
    if let Some(plan) = download_plan() {
        assert_eq!(plan.version, pins::VERSION);
        assert!(plan.url.starts_with("https://storage.googleapis.com/"));
    }
}

#[test]
fn a_plan_without_a_real_hash_is_refused() {
    assert!(!valid_sha256("TODO"));
    assert!(!valid_sha256(&"g".repeat(64)));
    assert!(valid_sha256(&"a".repeat(64)));
}

#[tokio::test]
async fn a_good_archive_is_downloaded_verified_and_unpacked() {
    let body = fake_archive();
    let (listener, addr) = bind().await;
    let _server = serve(listener, body.clone(), None);
    let dir = tempfile::tempdir().unwrap();
    let steps = Arc::new(Mutex::new(Vec::new()));
    let seen = steps.clone();
    let progress = move |p: DownloadProgress| seen.lock().unwrap().push(p.stage);
    let info = download(
        &plan(addr, &body, leak(sha(&body))),
        dir.path(),
        &progress,
        &CancellationFlag::new(),
    )
    .await
    .unwrap();
    assert_eq!(info.source, BrowserSource::Downloaded);
    assert_eq!(info.version, "155.0.8059.39");
    assert!(info.path.starts_with(dir.path().join("browsers/155.0.8059.39")));
    assert!(
        !dir.path().join("browsers/.download").exists(),
        "the archive is deleted"
    );
    let steps = steps.lock().unwrap();
    for stage in [
        DownloadStage::Download,
        DownloadStage::Verify,
        DownloadStage::Unpack,
    ] {
        assert!(steps.contains(&stage), "{stage:?}");
    }
    // Found again later without downloading.
    let found = crate::find::find_downloaded(dir.path()).unwrap();
    assert_eq!(found.path, info.path);
}

#[tokio::test]
async fn a_hash_mismatch_removes_everything() {
    let body = fake_archive();
    let (listener, addr) = bind().await;
    let _server = serve(listener, body.clone(), None);
    let dir = tempfile::tempdir().unwrap();
    let wrong = leak("0".repeat(64));
    let err = download(
        &plan(addr, &body, wrong),
        dir.path(),
        &no_progress(),
        &CancellationFlag::new(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, DownloadError::Checksum), "{err:?}");
    let left: Vec<_> = std::fs::read_dir(dir.path().join("browsers"))
        .map(|d| d.filter_map(Result::ok).map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "left behind: {left:?}");
}

#[tokio::test]
async fn cancel_removes_the_partial_files() {
    let body = fake_archive();
    let (listener, addr) = bind().await;
    let _server = serve(listener, body.clone(), None);
    let dir = tempfile::tempdir().unwrap();
    let cancel = CancellationFlag::new();
    let flag = cancel.clone();
    let progress = move |p: DownloadProgress| {
        if p.done_bytes > 0 {
            flag.cancel();
        }
    };
    let err = download(
        &plan(addr, &body, leak(sha(&body))),
        dir.path(),
        &progress,
        &cancel,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, DownloadError::Cancelled), "{err:?}");
    let left: Vec<_> = std::fs::read_dir(dir.path().join("browsers"))
        .map(|d| d.filter_map(Result::ok).map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "left behind: {left:?}");
}

#[tokio::test]
async fn offline_fails_clearly_then_works_when_the_connection_is_back() {
    let body = fake_archive();
    // Reserve a port and close it: connections are refused.
    let (listener, addr) = bind().await;
    drop(listener);
    let client = client().unwrap();
    assert!(!reachable(&client, &format!("http://{addr}/")).await);
    let dir = tempfile::tempdir().unwrap();
    let p = plan(addr, &body, leak(sha(&body)));
    let err = download_with(&p, dir.path(), &fast(), &no_progress(), &CancellationFlag::new())
        .await
        .unwrap_err();
    assert!(matches!(err, DownloadError::Network(_)), "{err:?}");
    assert!(err.to_string().starts_with("No internet connection"));

    // The server comes up on the same port.
    let listener = TcpListener::bind(addr).await.unwrap();
    let _server = serve(listener, body.clone(), None);
    assert!(reachable(&client, &format!("http://{addr}/")).await);
    let info = download_with(&p, dir.path(), &fast(), &no_progress(), &CancellationFlag::new())
        .await
        .unwrap();
    assert_eq!(info.source, BrowserSource::Downloaded);
}

#[tokio::test]
async fn a_connection_dropped_mid_file_resumes_from_the_bytes_written() {
    let body = fake_archive();
    let cut = 100_000;
    assert!(body.len() > cut);
    let (listener, addr) = bind().await;
    let server = serve(listener, body.clone(), Some(cut));
    let dir = tempfile::tempdir().unwrap();
    let waited = Arc::new(AtomicBool::new(false));
    let flag = waited.clone();
    let progress = move |p: DownloadProgress| {
        if p.waiting {
            flag.store(true, Ordering::Relaxed);
        }
    };
    let info = download_with(
        &plan(addr, &body, leak(sha(&body))),
        dir.path(),
        &fast(),
        &progress,
        &CancellationFlag::new(),
    )
    .await
    .unwrap();
    assert!(
        waited.load(Ordering::Relaxed),
        "the dialog is told the connection was lost"
    );
    // The second request asked only for the rest; the hash covers the whole.
    assert!(
        server.ranges.lock().unwrap().contains(&format!("{cut}-")),
        "{:?}",
        server.ranges.lock().unwrap()
    );
    assert_eq!(info.version, "155.0.8059.39");
}

#[tokio::test]
async fn giving_up_keeps_the_partial_file_for_a_later_resume() {
    let body = fake_archive();
    let (listener, addr) = bind().await;
    let server = serve(listener, body.clone(), Some(50_000));
    let dir = tempfile::tempdir().unwrap();
    let p = plan(addr, &body, leak(sha(&body)));
    // After the cut the server goes away for good.
    let stop = server.task.abort_handle();
    let progress = move |p: DownloadProgress| {
        if p.waiting {
            stop.abort();
        }
    };
    let opts = DownloadOptions {
        retry_every: Duration::from_millis(50),
        give_up_after: Some(Duration::from_millis(300)),
    };
    let err = download_with(&p, dir.path(), &opts, &progress, &CancellationFlag::new())
        .await
        .unwrap_err();
    assert!(matches!(err, DownloadError::ConnectionLost), "{err:?}");
    let part = dir
        .path()
        .join("browsers/.download/155.0.8059.39-linux64.zip.part");
    assert_eq!(std::fs::metadata(part).unwrap().len(), 50_000);
}
