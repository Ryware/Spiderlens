//! Static accessibility checks on a finished crawl (spec 026 FR-031, T061):
//! the stored HTML is parsed again with the checks on and the results are
//! written the way a crawl writes them. Nothing is fetched.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use spiderlens_config::{AccessibilitySettings, StoredCrawlConfig};
use url::Url;

use crate::CheckOptions;
use crate::crawl::store::custom::StoredBody;
use crate::crawl::store::{A11yNodeRow, A11yRow, MobileRow, StoreError};
use crate::crawl::{CrawlStore, open_for_spelling};
use crate::extract::a11y::{A11yStatic, StaticCheck};
use crate::extract::{PageInput, extract_page};

/// Pages read and written per transaction.
const BATCH: usize = 1_000;

/// What a re-check did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RerunSummary {
    /// Pages checked.
    pub pages: u64,
    /// Checked pages with at least one finding.
    pub with_findings: u64,
    /// Crawled pages that have no stored HTML (or whose HTML could not be
    /// read); their results are left as they were.
    pub skipped_no_html: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum AccessibilityError {
    /// The checks cannot run on this file (still crawling, in use, no stored
    /// HTML).
    #[error("{0}")]
    NotPossible(String),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Runs the static checks on every page of a crawl that has stored HTML and
/// replaces the page's static results. `progress(done, total)` is called
/// after every batch. Refuses a file another process holds (the file is
/// then view-only for this one), a crawl that is still running and a crawl
/// without stored HTML.
pub fn rerun_static(
    path: &Path,
    settings: &AccessibilitySettings,
    progress: impl FnMut(u64, u64),
) -> Result<RerunSummary, AccessibilityError> {
    rerun_static_cancellable(path, settings, &AtomicBool::new(false), progress)
}

/// [`rerun_static`] that stops after the current batch when `cancel` is set.
/// Pages already written keep their results.
pub fn rerun_static_cancellable(
    path: &Path,
    settings: &AccessibilitySettings,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<RerunSummary, AccessibilityError> {
    let (_lock, store) = open_for_spelling(path).map_err(|e| match e {
        crate::crawl::file::FileError::Store(s) => AccessibilityError::Store(s),
        other => AccessibilityError::NotPossible(format!(
            "{other}. The file is open view-only; close the other window or process, then check again."
        )),
    })?;
    if store.meta("status")?.as_deref() == Some("running") {
        return Err(AccessibilityError::NotPossible(
            "The crawl is still running. Pause or stop it first, then check accessibility.".into(),
        ));
    }
    let (extracted, with_body) = store.extracted_pages()?;
    if with_body == 0 {
        return Err(AccessibilityError::NotPossible(
            "This crawl has no stored page HTML, so accessibility can only be checked in a new crawl \
             (turn on Store page HTML)."
                .into(),
        ));
    }
    let token = store
        .meta("config")?
        .and_then(|c| serde_json::from_str::<StoredCrawlConfig>(&c).ok())
        .map(|s| CheckOptions::from(&s.effective()).robots_token)
        .unwrap_or_else(|| "SpiderLens".into());
    let total = extracted.max(0) as u64;

    let workers = std::thread::available_parallelism().map_or(2, |n| n.get());
    let mut summary = RerunSummary {
        pages: 0,
        with_findings: 0,
        skipped_no_html: store.extracted_without_body()?.len() as u64,
    };
    let mut after = 0i64;
    while !cancel.load(Ordering::Relaxed) {
        let batch = store.bodies_after(after, BATCH)?;
        let Some(last) = batch.last() else { break };
        after = last.url_id;
        let checked = check_batch(&batch, &token, workers);
        store.begin()?;
        for (id, found) in &checked {
            match found {
                Some(a) => {
                    write_static(&store, *id, a, settings)?;
                    summary.pages += 1;
                    if !a.findings.is_empty() {
                        summary.with_findings += 1;
                    }
                }
                None => summary.skipped_no_html += 1,
            }
        }
        store.commit()?;
        progress(summary.pages, total.max(summary.pages));
    }
    if summary.pages > 0 {
        store.add_data_features(&["accessibility_static"])?;
    }
    Ok(summary)
}

/// Extracts a batch on several threads; None for a page that cannot be read.
fn check_batch(batch: &[StoredBody], token: &str, workers: usize) -> Vec<(i64, Option<A11yStatic>)> {
    let per = batch.len().div_ceil(workers.max(1)).max(1);
    std::thread::scope(|scope| {
        let handles: Vec<_> = batch
            .chunks(per)
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|page| (page.url_id, check_stored(page, token)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    })
}

/// The static checks on one stored page, exactly as during the crawl.
fn check_stored(page: &StoredBody, token: &str) -> Option<A11yStatic> {
    let body = zstd::stream::decode_all(page.packed.as_slice()).ok()?;
    let url = Url::parse(&page.url).ok()?;
    let input = PageInput {
        url: &url,
        charset: page.encoding.as_deref(),
        body: &body,
        truncated: page.partial,
        robots_token: token,
        content: None,
        a11y: true,
        hook: None,
        keep_text: false,
    };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| extract_page(&input)))
        .ok()
        .and_then(|d| d.a11y)
}

/// Writes one page's static results, state and mobile facts.
fn write_static(
    store: &CrawlStore,
    url_id: i64,
    a: &A11yStatic,
    settings: &AccessibilitySettings,
) -> Result<(), StoreError> {
    let rows: Vec<A11yRow<'_>> = a
        .findings
        .iter()
        .map(|f| A11yRow {
            check_id: format!("static.{}", f.check.as_str()).into(),
            kind: "violation",
            source: "static",
            impact: Some(f.check.impact().as_str()),
            wcag: f.check.wcag().into(),
            nodes_total: f.count,
            nodes: f
                .nodes
                .iter()
                .map(|n| A11yNodeRow {
                    selector: n.selector.as_str().into(),
                    excerpt: n.excerpt.as_str().into(),
                    data: None,
                    bbox: None,
                })
                .collect(),
        })
        .collect();
    store.record_a11y(url_id, "static", &rows)?;
    store.set_a11y_state(url_id, "static", "checked", None, None)?;
    if settings.mobile {
        let viewport_state = match &a.viewport {
            None => "missing",
            Some(v) if v.zoom_disabled => "zoom_disabled",
            Some(v) if !v.device_width => "not_device_width",
            Some(_) => "ok",
        };
        store.record_mobile(
            url_id,
            &MobileRow {
                viewport: a.viewport.as_ref().map(|v| v.content.as_str()),
                viewport_state: Some(viewport_state),
                plugins: Some(
                    a.finding(StaticCheck::PluginElement)
                        .map_or(0, |f| i64::from(f.count)),
                ),
                ..MobileRow::default()
            },
        )?;
    }
    Ok(())
}
