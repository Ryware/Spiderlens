//! Downloading the pinned `chrome-headless-shell` (FR-045, research R14).
//!
//! The browser is not bundled: it is downloaded on first use, after the
//! user agreed, from Chrome for Testing over HTTPS. The archive must match
//! the SHA-256 pinned in [`pins`] or everything is removed. A connection
//! that drops mid-download does not fail it: the download waits, checks the
//! host again every few seconds and resumes with an HTTP `Range` request
//! from the bytes already on disk (owner requirement, guide section 7).
//! The hash is checked over the whole file at the end. Cancel removes the
//! partial files.

pub mod pins;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::types::{
    BrowserInfo, CancellationFlag, DownloadError, DownloadPlan, DownloadProgress, DownloadStage, NotUsable,
};

/// The pinned download for this platform, or `None` where Chrome for
/// Testing has none (32-bit Windows, Linux on ARM).
pub fn download_plan() -> Option<DownloadPlan> {
    let platform = pins::current_platform()?;
    let pin = pins::pin_for(platform)?;
    Some(DownloadPlan {
        version: pins::VERSION,
        platform: pin.platform,
        url: pins::archive_url(platform),
        bytes: pin.bytes,
        sha256: pin.sha256,
    })
}

/// How a download behaves when the connection is lost.
#[derive(Clone, Debug)]
pub struct DownloadOptions {
    /// How long to wait between reachability checks while the connection is
    /// lost (5 s).
    pub retry_every: Duration,
    /// Stop waiting after this long (the bytes written are kept). `None`
    /// waits until the connection returns or the caller cancels.
    pub give_up_after: Option<Duration>,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            retry_every: Duration::from_secs(5),
            give_up_after: None,
        }
    }
}

/// How often progress is reported while bytes arrive.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// Downloads, verifies and unpacks into `<data_dir>/browsers/`. The caller
/// must have the user's consent (FR-045). Partial files are removed on
/// error or cancel, except when the connection is lost (they are kept to
/// resume).
pub async fn download(
    plan: &DownloadPlan,
    data_dir: &Path,
    progress: &(dyn Fn(DownloadProgress) + Send + Sync),
    cancel: &CancellationFlag,
) -> Result<BrowserInfo, DownloadError> {
    download_with(plan, data_dir, &DownloadOptions::default(), progress, cancel).await
}

/// [`download`] with the waiting rules given.
pub async fn download_with(
    plan: &DownloadPlan,
    data_dir: &Path,
    opts: &DownloadOptions,
    progress: &(dyn Fn(DownloadProgress) + Send + Sync),
    cancel: &CancellationFlag,
) -> Result<BrowserInfo, DownloadError> {
    // A plan without a real hash is refused: nothing unverifiable is run.
    if !valid_sha256(plan.sha256) {
        return Err(DownloadError::NoPlan);
    }
    let root = data_dir.join("browsers");
    let work = root.join(".download");
    let part = work.join(format!("{}-{}.zip.part", plan.version, plan.platform));
    match run(plan, data_dir, &root, &work, &part, opts, progress, cancel).await {
        Ok(info) => {
            let _ = std::fs::remove_dir_all(&work);
            Ok(info)
        }
        // Kept for a later resume.
        Err(e @ (DownloadError::ConnectionLost | DownloadError::Network(_))) => Err(e),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&work);
            Err(e)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run(
    plan: &DownloadPlan,
    data_dir: &Path,
    root: &Path,
    work: &Path,
    part: &Path,
    opts: &DownloadOptions,
    progress: &(dyn Fn(DownloadProgress) + Send + Sync),
    cancel: &CancellationFlag,
) -> Result<BrowserInfo, DownloadError> {
    tokio::fs::create_dir_all(work).await.map_err(DownloadError::Io)?;
    let total = fetch(plan, part, opts, progress, cancel).await?;
    if cancel.is_cancelled() {
        return Err(DownloadError::Cancelled);
    }

    // The hash covers the whole file, however many tries it took.
    let report = |stage, done| {
        progress(DownloadProgress {
            stage,
            done_bytes: done,
            total_bytes: total,
            waiting: false,
        });
    };
    report(DownloadStage::Verify, 0);
    let mut file = tokio::fs::File::open(part).await.map_err(DownloadError::Io)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut done = 0u64;
    loop {
        let n = file.read(&mut buf).await.map_err(DownloadError::Io)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        done += n as u64;
        if done % (8 << 20) < n as u64 {
            report(DownloadStage::Verify, done);
        }
        if cancel.is_cancelled() {
            return Err(DownloadError::Cancelled);
        }
    }
    drop(file);
    if !hex(&hasher.finalize()).eq_ignore_ascii_case(plan.sha256) {
        return Err(DownloadError::Checksum);
    }

    report(DownloadStage::Unpack, 0);
    let tmp = root.join(format!(".unpack-{}", plan.version));
    let final_dir = root.join(plan.version);
    let (archive, tmp2, final2) = (part.to_path_buf(), tmp.clone(), final_dir.clone());
    let unpacked = tokio::task::spawn_blocking(move || {
        let _ = std::fs::remove_dir_all(&tmp2);
        unpack(&archive, &tmp2)?;
        let _ = std::fs::remove_dir_all(&final2);
        std::fs::rename(&tmp2, &final2).map_err(DownloadError::Io)
    })
    .await
    .map_err(|e| DownloadError::Unusable(e.to_string()))?;
    if let Err(e) = unpacked {
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&final_dir);
        return Err(e);
    }
    report(DownloadStage::Unpack, total);
    crate::find::find_downloaded(data_dir).map_err(|e| {
        let _ = std::fs::remove_dir_all(&final_dir);
        DownloadError::Unusable(match e {
            NotUsable::NotFound => "the archive holds no browser".to_owned(),
            NotUsable::TooOld { found, min_major } => {
                format!("version {} is older than {min_major}", found.version)
            }
            NotUsable::Broken { message, .. } => message,
        })
    })
}

/// Writes the archive to `part`, resuming from what is already there.
/// Returns the archive's size.
async fn fetch(
    plan: &DownloadPlan,
    part: &Path,
    opts: &DownloadOptions,
    progress: &(dyn Fn(DownloadProgress) + Send + Sync),
    cancel: &CancellationFlag,
) -> Result<u64, DownloadError> {
    let client = client()?;
    let mut total = plan.bytes;
    let mut lost_since: Option<Instant> = None;
    let mut last_report: Option<Instant> = None;
    let emit = |done: u64, total: u64, waiting: bool| {
        progress(DownloadProgress {
            stage: DownloadStage::Download,
            done_bytes: done,
            total_bytes: total,
            waiting,
        });
    };
    loop {
        if cancel.is_cancelled() {
            return Err(DownloadError::Cancelled);
        }
        let have = tokio::fs::metadata(part).await.map_or(0, |m| m.len());
        if total > 0 && have >= total {
            return Ok(have);
        }
        let mut request = client.get(&plan.url);
        if have > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let mut response = match request.send().await {
            Ok(r) => r,
            Err(e) => {
                wait_for_connection(
                    &client,
                    plan,
                    opts,
                    cancel,
                    have,
                    total,
                    &mut lost_since,
                    &emit,
                    &e.to_string(),
                )
                .await?;
                continue;
            }
        };
        let status = response.status().as_u16();
        if status == 416 {
            // Nothing left to send: what is on disk is the whole file.
            return Ok(have);
        }
        if status != 200 && status != 206 {
            return Err(DownloadError::Network(format!("HTTP {status}")));
        }
        let resumed = status == 206 && have > 0;
        let mut done = if resumed { have } else { 0 };
        if let Some(len) = response.content_length() {
            total = done + len;
        }
        let mut file = if resumed {
            tokio::fs::OpenOptions::new().append(true).open(part).await
        } else {
            // A server that ignores Range starts over from the first byte.
            tokio::fs::File::create(part).await
        }
        .map_err(DownloadError::Io)?;
        lost_since = None;
        let broke = loop {
            if cancel.is_cancelled() {
                return Err(DownloadError::Cancelled);
            }
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    file.write_all(&chunk).await.map_err(DownloadError::Io)?;
                    done += chunk.len() as u64;
                    if last_report.is_none_or(|t| t.elapsed() >= PROGRESS_EVERY) {
                        last_report = Some(Instant::now());
                        emit(done, total, false);
                    }
                }
                Ok(None) => break None,
                Err(e) => break Some(e.to_string()),
            }
        };
        file.flush().await.map_err(DownloadError::Io)?;
        drop(file);
        match broke {
            None => {
                emit(done, total.max(done), false);
                return Ok(done);
            }
            Some(message) => {
                wait_for_connection(
                    &client,
                    plan,
                    opts,
                    cancel,
                    done,
                    total,
                    &mut lost_since,
                    &emit,
                    &message,
                )
                .await?;
            }
        }
    }
}

/// Called when a request or the body stream failed. With nothing on disk
/// the download fails at once ("no internet"); otherwise it waits, polling
/// the host, until the connection is back (then the caller resumes).
#[allow(clippy::too_many_arguments)]
async fn wait_for_connection(
    client: &reqwest::Client,
    plan: &DownloadPlan,
    opts: &DownloadOptions,
    cancel: &CancellationFlag,
    on_disk: u64,
    total: u64,
    lost_since: &mut Option<Instant>,
    emit: &dyn Fn(u64, u64, bool),
    message: &str,
) -> Result<(), DownloadError> {
    if on_disk == 0 && lost_since.is_none() {
        return Err(DownloadError::Network(message.to_owned()));
    }
    let since = *lost_since.get_or_insert_with(Instant::now);
    loop {
        emit(on_disk, total, true);
        let until = Instant::now() + opts.retry_every;
        while Instant::now() < until {
            if cancel.is_cancelled() {
                return Err(DownloadError::Cancelled);
            }
            tokio::time::sleep(Duration::from_millis(50).min(opts.retry_every)).await;
        }
        if opts.give_up_after.is_some_and(|limit| since.elapsed() > limit) {
            return Err(DownloadError::ConnectionLost);
        }
        if reachable(client, &plan.url).await {
            return Ok(());
        }
    }
}

fn client() -> Result<reqwest::Client, DownloadError> {
    reqwest::Client::builder()
        .user_agent("SpiderLens")
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| DownloadError::Network(e.to_string()))
}

/// A cheap HEAD request: any HTTP answer means the host can be reached.
async fn reachable(client: &reqwest::Client, url: &str) -> bool {
    client
        .head(url)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .is_ok()
}

/// Whether the download host answers right now (a HEAD request). The
/// desktop app polls this every 5 s while the browser dialog is open.
pub async fn download_host_reachable() -> bool {
    match client() {
        Ok(c) => reachable(&c, &format!("https://{}/", pins::HOST)).await,
        Err(_) => false,
    }
}

fn valid_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

fn unpack(archive: &Path, dest: &Path) -> Result<(), DownloadError> {
    let bad = |e: zip::result::ZipError| DownloadError::Unusable(e.to_string());
    let file = std::fs::File::open(archive).map_err(DownloadError::Io)?;
    let mut zip = zip::ZipArchive::new(file).map_err(bad)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(bad)?;
        let Some(relative) = entry.enclosed_name() else {
            return Err(DownloadError::Unusable("the archive has an unsafe path".into()));
        };
        let out: PathBuf = dest.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(DownloadError::Io)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(DownloadError::Io)?;
        }
        let mut f = std::fs::File::create(&out).map_err(DownloadError::Io)?;
        std::io::copy(&mut entry, &mut f).map_err(DownloadError::Io)?;
        f.flush().map_err(DownloadError::Io)?;
        drop(f);
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode & 0o777))
                .map_err(DownloadError::Io)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
