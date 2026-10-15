//! Running rules on a finished crawl from its stored HTML (plan D8,
//! FR-012 to FR-015). Nothing is fetched.
//!
//! The main thread owns the crawl file: it reads stored bodies in id order
//! and writes results in batches; worker threads decompress, parse and
//! apply the rules with the same code the crawl uses, so values are
//! identical (FR-015).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Instant;

use spiderlens_config::{CustomRules, StoredCrawlConfig};
use url::Url;

use super::{CompiledRules, PageRules, compile};
use crate::CheckOptions;
use crate::crawl::CrawlStore;
use crate::crawl::store::StoreError;
use crate::crawl::store::custom::{Slots, StoredBody};
use crate::extract::{PageInput, extract_page_with};

/// Pages read and written per transaction.
const BATCH: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct RerunProgress {
    pub done: u64,
    pub total: u64,
    pub per_sec: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RerunState {
    Complete,
    Partial,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RerunReport {
    pub state: RerunState,
    pub pages: u64,
    /// Pages with no stored HTML, recorded as not available.
    pub not_available: u64,
    pub rules: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum RerunError {
    /// The rules cannot run on this file (no stored HTML, still crawling,
    /// no rules, invalid rules).
    #[error("{0}")]
    NotPossible(String),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// What to run.
pub enum Which<'a> {
    /// The rules stored with the crawl.
    Stored,
    /// These rules: stored with the crawl first (new ones added, changed
    /// ones updated), then run.
    These(&'a CustomRules),
}

/// Runs rules on every page of a crawl with stored HTML. `only` limits the
/// run to rules with these names. `progress` is called after every batch;
/// setting `cancel` stops after the current batch and records a partial
/// run (FR-014).
pub fn rerun(
    path: &Path,
    which: Which<'_>,
    only: &[String],
    cancel: &AtomicBool,
    mut progress: impl FnMut(RerunProgress),
) -> Result<RerunReport, RerunError> {
    let store = CrawlStore::open(path)?;
    if store.meta("status")?.as_deref() == Some("running") {
        return Err(RerunError::NotPossible(
            "The crawl is still running. Pause or stop it first, then run the rules.".into(),
        ));
    }
    let with_body = store.extracted_pages()?.1;
    if with_body == 0 {
        return Err(RerunError::NotPossible(
            "This crawl has no stored page HTML, so rules can only run in a new crawl \
             (turn on Store page HTML)."
                .into(),
        ));
    }
    if let Which::These(rules) = which {
        store.sync_custom_rules(rules)?;
    }
    let (all, all_slots) = store.stored_rule_set()?;
    let (set, slots) = select(&all, &all_slots, only);
    if set.is_empty() {
        return Err(RerunError::NotPossible(if only.is_empty() {
            "There are no rules to run.".into()
        } else {
            format!("No rule is named {}.", only.join(", "))
        }));
    }
    let compiled = match compile(&set) {
        Ok(Some(c)) => Arc::new(c),
        Ok(None) => unreachable!("set is not empty"),
        Err(errors) => {
            let joined = errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ");
            return Err(RerunError::NotPossible(format!("custom rules: {joined}")));
        }
    };
    let token = store
        .meta("config")?
        .and_then(|c| serde_json::from_str::<StoredCrawlConfig>(&c).ok())
        .map(|s| CheckOptions::from(&s.effective()).robots_token)
        .unwrap_or_else(|| "SpiderLens".into());
    let missing = store.extracted_without_body()?;
    let total = with_body as u64 + missing.len() as u64;

    let started = Instant::now();
    let mut done = 0u64;
    let mut cancelled = false;

    // Workers: one per core, fed through a shared queue.
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get());
    let (job_tx, job_rx) = mpsc::sync_channel::<StoredBody>(BATCH);
    let job_rx = Arc::new(Mutex::new(job_rx));
    let (res_tx, res_rx) = mpsc::channel::<(i64, PageRules)>();
    std::thread::scope(|scope| -> Result<(), RerunError> {
        for _ in 0..workers {
            let rx = Arc::clone(&job_rx);
            let tx = res_tx.clone();
            let rules = Arc::clone(&compiled);
            let token = token.clone();
            scope.spawn(move || {
                loop {
                    let job = rx.lock().expect("queue lock").recv();
                    let Ok(job) = job else { break };
                    let out = apply_stored(&rules, &job, &token);
                    if tx.send((job.url_id, out)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(res_tx);

        let mut after = 0i64;
        loop {
            if cancel.load(Ordering::Relaxed) {
                cancelled = true;
                break;
            }
            let batch = store.bodies_after(after, BATCH)?;
            let Some(last) = batch.last() else { break };
            after = last.url_id;
            let n = batch.len();
            // Feed while collecting, so the queue never blocks the reader.
            let mut results = Vec::with_capacity(n);
            let mut iter = batch.into_iter();
            while results.len() < n {
                match iter.next() {
                    Some(job) => {
                        job_tx.send(job).expect("workers run until the queue closes");
                        while let Ok(r) = res_rx.try_recv() {
                            results.push(r);
                        }
                    }
                    None => results.push(res_rx.recv().expect("each job gets a result")),
                }
            }
            store.begin()?;
            for (url_id, out) in &results {
                store.update_custom(*url_id, &slots, out)?;
            }
            store.commit()?;
            done += n as u64;
            progress(RerunProgress {
                done,
                total,
                per_sec: rate(done, started),
            });
        }
        drop(job_tx);
        Ok(())
    })?;

    if !cancelled && !missing.is_empty() {
        let na = compiled.not_available();
        store.begin()?;
        for id in &missing {
            store.update_custom(*id, &slots, &na)?;
        }
        store.commit()?;
        done += missing.len() as u64;
        progress(RerunProgress {
            done,
            total,
            per_sec: rate(done, started),
        });
    }
    let state = if cancelled {
        RerunState::Partial
    } else {
        RerunState::Complete
    };
    let all_slots: Vec<u32> = slots.extractors.iter().chain(&slots.searches).copied().collect();
    let state_str = match state {
        RerunState::Complete => "complete",
        RerunState::Partial => "partial",
    };
    store.mark_custom_run(&all_slots, state_str, Some(done as i64))?;
    Ok(RerunReport {
        state,
        pages: done,
        not_available: if cancelled { 0 } else { missing.len() as u64 },
        rules: all_slots.len(),
    })
}

fn rate(done: u64, started: Instant) -> u32 {
    let secs = started.elapsed().as_secs_f64().max(0.001);
    (done as f64 / secs) as u32
}

/// The named rules of a set (all of them when `only` is empty).
fn select(all: &CustomRules, slots: &Slots, only: &[String]) -> (CustomRules, Slots) {
    let keep = |name: &str| only.is_empty() || only.iter().any(|n| n.eq_ignore_ascii_case(name.trim()));
    let mut set = CustomRules::default();
    let mut out = Slots::default();
    for (e, s) in all.extractors.iter().zip(&slots.extractors) {
        if keep(&e.name) {
            set.extractors.push(e.clone());
            out.extractors.push(*s);
        }
    }
    for (r, s) in all.searches.iter().zip(&slots.searches) {
        if keep(&r.name) {
            set.searches.push(r.clone());
            out.searches.push(*s);
        }
    }
    (set, out)
}

/// The rules on one stored page, exactly as during the crawl.
fn apply_stored(rules: &CompiledRules, page: &StoredBody, token: &str) -> PageRules {
    let Ok(body) = zstd::stream::decode_all(page.packed.as_slice()) else {
        return rules.not_available();
    };
    let Ok(url) = Url::parse(&page.url) else {
        return rules.not_available();
    };
    let input = PageInput {
        url: &url,
        charset: page.encoding.as_deref(),
        body: &body,
        truncated: page.partial,
        robots_token: token,
        a11y: false,
    };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        extract_page_with(&input, Some(rules))
    })) {
        Ok(d) => d.custom.unwrap_or_else(|| rules.not_available()),
        Err(_) => rules.not_available(),
    }
}

/// Applies rules to one page of a crawl from its stored HTML, for the
/// tester (FR-018). None when the page has no stored HTML.
pub fn test_stored_page(
    store: &CrawlStore,
    url_id: i64,
    rules: &CompiledRules,
) -> Result<Option<PageRules>, StoreError> {
    let Some(html) = store.page_html(url_id)? else {
        return Ok(None);
    };
    let url = Url::parse(&store.url_of(url_id)?).map_err(|e| StoreError::Format(e.to_string()))?;
    let (encoding, partial) = store.page_encoding(url_id)?;
    let d = extract_page_with(
        &PageInput {
            url: &url,
            charset: encoding.as_deref(),
            body: &html,
            truncated: partial,
            robots_token: "SpiderLens",
            a11y: false,
        },
        Some(rules),
    );
    Ok(d.custom)
}
