//! Headless Chromium rendering for SpiderLens (spec 014).
//!
//! Finds or downloads a Chromium-family browser and renders one page at a
//! time in a fresh browser context, answering every request the page makes
//! from the caller (the engine) rather than the network. It knows nothing
//! about crawls, crawl files or the UI, and depends on nothing in this
//! workspace. The public API is `contracts/render-crate.md`.
//!
//! The browser is a `chrome-headless-shell` downloaded on first use, after
//! the user agreed ([`fetcher`]). Not yet here: running probes (each probe
//! is reported as failed until specs 025 and 026 need them).

mod fetcher;
mod find;
mod guard;
mod launch;
mod page;
mod settle;
mod types;

use std::path::Path;
use std::sync::Arc;

pub use fetcher::{DownloadOptions, download, download_host_reachable, download_plan, download_with};
pub use find::{
    MAJOR_TOLERANCE, PINNED_VERSION, PREFER_DOWNLOAD, browser_for_tests, data_dir, find_browser, min_major,
    should_offer_download,
};
pub use guard::{GUARD_ARG, parse_guard_args, run_guard};
pub use launch::{default_concurrency, launch_count, sweep_stale_profiles, system_memory};
pub use types::*;

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
    info: BrowserInfo,
    opts: LaunchOptions,
    running: Option<launch::Running>,
    /// Set when a render hit the hard stop: the browser may be stuck.
    stuck: std::sync::atomic::AtomicBool,
}

impl Browser {
    /// Starts the browser. Stale temporary profiles of earlier, killed runs
    /// are deleted first.
    pub async fn launch(info: &BrowserInfo, opts: LaunchOptions) -> Result<Self, LaunchError> {
        let running = launch::Running::start(info, &opts).await?;
        Ok(Self {
            info: info.clone(),
            opts,
            running: Some(running),
            stuck: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// The browser this process runs.
    pub fn info(&self) -> &BrowserInfo {
        &self.info
    }

    /// Renders one page in a fresh browser context. Never panics; every
    /// problem is a [`RenderOutcome`] state.
    ///
    /// Returns within `job.max_time` plus 2 s even when the browser hangs
    /// (SC-007); the browser then reports [`Browser::needs_restart`].
    pub async fn render(&self, job: RenderJob, net: Arc<dyn Network>) -> RenderOutcome {
        let Some(running) = &self.running else {
            return RenderOutcome::failed(FailedReason::RendererError, true, job.window);
        };
        let (out, stuck) = page::render(&running.cdp, job, net, self.opts.heap_limit_mb).await;
        if stuck {
            self.stuck.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        out
    }

    /// Whether a render had to be abandoned at the hard stop, so the browser
    /// should be restarted before the next page.
    pub fn needs_restart(&self) -> bool {
        self.stuck.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Resident memory of the browser process tree, in bytes.
    pub fn memory_bytes(&self) -> u64 {
        self.running.as_ref().map_or(0, launch::Running::memory_bytes)
    }

    /// Closes the browser and starts a new one with the same options.
    pub async fn restart(&mut self) -> Result<(), LaunchError> {
        if let Some(r) = self.running.take() {
            r.close().await;
        }
        self.running = Some(launch::Running::start(&self.info, &self.opts).await?);
        self.stuck.store(false, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Closes the process and deletes its temporary profile. Also on drop.
    pub async fn close(mut self) {
        if let Some(r) = self.running.take() {
            r.close().await;
        }
    }

    /// The temporary profile folder of the running browser. For tests.
    #[doc(hidden)]
    pub fn profile_dir(&self) -> Option<&std::path::Path> {
        self.running.as_ref().map(launch::Running::profile)
    }

    /// Opens `about:blank` in a fresh context and evaluates a script, to
    /// check the connection works. For tests.
    #[doc(hidden)]
    pub async fn smoke_test(&self) -> Result<String, String> {
        use chromiumoxide::cdp::browser_protocol::target::{
            CreateBrowserContextParams, CreateTargetParams, DisposeBrowserContextParams,
        };
        let cdp = &self.running.as_ref().ok_or("closed")?.cdp;
        let context = cdp
            .execute(CreateBrowserContextParams::default())
            .await
            .map_err(|e| e.to_string())?
            .result
            .browser_context_id;
        let mut target = CreateTargetParams::new("about:blank");
        target.browser_context_id = Some(context.clone());
        let result = async {
            let page = cdp.new_page(target).await.map_err(|e| e.to_string())?;
            let v = page
                .evaluate("navigator.language + ' ' + (6 * 7)")
                .await
                .map_err(|e| e.to_string())?
                .into_value::<String>()
                .map_err(|e| e.to_string())?;
            page.close().await.ok();
            Ok(v)
        }
        .await;
        cdp.execute(DisposeBrowserContextParams::new(context)).await.ok();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

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
