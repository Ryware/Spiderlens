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

mod find;
pub mod settle;
mod types;

use std::ffi::OsString;
use std::path::Path;
use std::sync::Arc;

pub use find::{
    MAJOR_TOLERANCE, PINNED_VERSION, PREFER_DOWNLOAD, data_dir, find_browser, min_major,
    should_offer_download,
};
pub use types::*;

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
    use url::Url;

    #[test]
    fn no_download_plan_until_the_pins_exist() {
        assert!(download_plan().is_none());
    }

    #[test]
    fn remove_download_without_a_download_is_ok() {
        let dir = std::env::temp_dir().join("spiderlens-render-test-no-download");
        assert!(remove_download(&dir).is_ok());
    }

    #[test]
    fn a_new_job_has_the_documented_defaults() {
        let url = Url::parse("http://js.test/a").expect("url");
        let main = MainResponse {
            status: 200,
            headers: Vec::new(),
            body: b"<html></html>".to_vec(),
        };
        let window = Window {
            width: 412,
            height: 732,
            mobile: true,
        };
        let job = RenderJob::new(url, main, "SpiderLens".into(), window);
        assert!(job.probes.is_empty());
        assert_eq!(job.probes_max_total, PROBES_MAX_TOTAL_DEFAULT);
        assert!(job.load_types.is_none());
        assert!(job.cookies.is_empty());
        assert!(job.proxy.is_none());
        assert_eq!(job.max_time, std::time::Duration::from_secs(10));
        let block = job.block;
        assert!(block.images && block.media && block.fonts);
        assert!(!block.stylesheets && !block.third_party);
        let internal = &job.internal_hosts;
        assert!(internal(&Url::parse("http://js.test/other").expect("url")));
        assert!(!internal(&Url::parse("http://cdn.test/lib.js").expect("url")));
    }

    #[test]
    fn a_failed_outcome_has_no_html_and_no_probes() {
        let window = Window {
            width: 1366,
            height: 768,
            mobile: false,
        };
        let out = RenderOutcome::failed(FailedReason::Crash, true, window);
        assert!(out.html.is_none());
        assert!(out.probes.is_empty());
        assert_eq!(
            out.state,
            RenderState::Failed {
                reason: FailedReason::Crash,
                retryable: true
            }
        );
    }

    #[test]
    fn cancellation_flag_is_shared_between_clones() {
        let flag = CancellationFlag::new();
        let other = flag.clone();
        other.cancel();
        assert!(flag.is_cancelled());
    }
}
