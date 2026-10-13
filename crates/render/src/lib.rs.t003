//! Headless Chromium rendering for SpiderLens (spec 014).
//!
//! Finds or downloads a Chromium-family browser and renders one page at a
//! time in a fresh browser context, answering every request the page makes
//! from the caller (the engine) rather than the network. It knows nothing
//! about crawls, crawl files or the UI, and depends on nothing in this
//! workspace. The public API is `contracts/render-crate.md`.
//!
//! This is the skeleton from task T003: the types are final, the functions
//! report "no browser" until browser discovery (T009), launch (T010) and
//! rendering (T016) land.

mod types;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use types::*;

/// User path, then a previous download, then installed Chrome, Chromium, Edge.
/// Runs `<browser> --version` (or reads the version resource on Windows).
///
/// Until T009 this always reports [`NotUsable::NotFound`].
pub fn find_browser(opts: &FindOptions) -> Result<BrowserInfo, NotUsable> {
    let _ = opts;
    Err(NotUsable::NotFound)
}

/// The SpiderLens data folder shared by the CLI and the desktop app. `None`
/// until T009.
pub fn data_dir() -> Option<PathBuf> {
    None
}

/// The pinned download for this platform, or `None` where Chrome for
/// Testing has none. `None` until the pins exist (research R14).
pub fn download_plan() -> Option<DownloadPlan> {
    None
}

/// Downloads, verifies and unpacks into `<data_dir>/browsers/`. The caller
/// must have the user's consent (FR-045). Partial files are removed on error
/// or cancel.
pub async fn download(
    plan: &DownloadPlan,
    data_dir: &Path,
    progress: &(dyn Fn(DownloadProgress) + Send + Sync),
    cancel: &CancellationFlag,
) -> Result<BrowserInfo, DownloadError> {
    let _ = (plan, data_dir, progress, cancel);
    Err(DownloadError::NoPlan)
}

/// Removes a previous download, if any.
pub fn remove_download(data_dir: &Path) -> std::io::Result<()> {
    let dir = data_dir.join("browsers");
    match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// One browser process.
pub struct Browser {
    window: Window,
}

impl Browser {
    /// Starts the browser. Until T010 this reports that no browser is usable.
    pub async fn launch(info: &BrowserInfo, opts: LaunchOptions) -> Result<Self, LaunchError> {
        let _ = (info, opts);
        Err(LaunchError::NotUsable(NotUsable::NotFound))
    }

    /// Renders one page in a fresh browser context. Never panics; every
    /// problem is a [`RenderOutcome`] state.
    pub async fn render(&self, job: RenderJob, net: Arc<dyn Network>) -> RenderOutcome {
        let _ = net;
        let _ = self.window;
        RenderOutcome::failed(FailedReason::RendererError, false, job.window)
    }

    /// Resident memory of the browser process tree, in bytes.
    pub fn memory_bytes(&self) -> u64 {
        0
    }

    pub async fn restart(&mut self) -> Result<(), LaunchError> {
        Err(LaunchError::NotUsable(NotUsable::NotFound))
    }

    /// Closes the process and deletes its temporary profile. Also on drop.
    pub async fn close(self) {}
}

/// The guard process entry point (research R12), for the hidden
/// `--render-guard <pid>` argument of the `spiderlens` and desktop binaries.
/// Until T010 it exits at once with status 2.
pub fn run_guard(parent_pid: u32, browser_args: Vec<OsString>) -> ! {
    let _ = (parent_pid, browser_args);
    std::process::exit(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_reports_no_browser() {
        let opts = FindOptions {
            user_path: None,
            data_dir: std::env::temp_dir(),
        };
        assert_eq!(find_browser(&opts), Err(NotUsable::NotFound));
        assert!(download_plan().is_none());
    }

    #[test]
    fn remove_download_without_a_download_is_ok() {
        let dir = std::env::temp_dir().join("spiderlens-render-test-no-download");
        assert!(remove_download(&dir).is_ok());
    }

    #[test]
    fn cancellation_flag_is_shared_between_clones() {
        let flag = CancellationFlag::new();
        let other = flag.clone();
        other.cancel();
        assert!(flag.is_cancelled());
    }
}
