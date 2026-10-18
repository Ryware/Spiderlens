//! What the two page experience providers share (spec 024 US3): choosing the
//! pages, a resumable queue of pages inside the crawl file, and a driver that
//! works through the queue with the provider's rate, concurrency and daily
//! allowance (plan D5, D7).
//!
//! Field data (`crux`) and lab tests (`psi`) are URL based: one request or a
//! few per page. Each page is settled in its own small transaction together
//! with the run's counts, so a paused, cancelled or interrupted run resumes
//! with exactly the pages still waiting.

use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::task::Poll;

use jiff::Timestamp;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::http::HttpError;
use crate::limits::{Daily, GOOGLE_TZ, Ledger, Limits, next_reset, provider_day};
use crate::matching::Matcher;
use crate::run::{Control, Ctx};
use crate::store::{self, Counts, RunProgress, RunState};
use crate::{Error, Provider, Result};

/// Internal pages that answered 2xx with HTML: the pages both providers
/// can say something about.
const HTML_PAGES: &str = "u.internal = 1 AND u.state = 'crawled' AND u.error_class IS NULL \
     AND u.status BETWEEN 200 AND 299 AND lower(COALESCE(u.content_type, '')) LIKE '%html%'";

/// Pages a search engine may index: no noindex and not canonical elsewhere.
const INDEXABLE: &str =
    "COALESCE(u.noindex, 0) = 0 AND (u.canonical IS NULL OR u.canonical = '' OR u.canonical = u.url)";

/// Pages taken from the queue per round.
const CHUNK: i64 = 100;

/// Which crawled pages a run covers (FR-020 URL set rule).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum UrlSet {
    /// Every internal HTML page that answered 2xx.
    All,
    /// The pages of `All` that are not noindex and are their own canonical.
    Indexable,
    /// The pages of `All` whose address starts with `prefix`.
    Prefix { prefix: String },
    /// These addresses, matched to crawled pages the way provider URLs are.
    List { urls: Vec<String> },
}

/// The pages a request covers, shallowest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub ids: Vec<i64>,
    /// Addresses of a `List` that are not crawled HTML pages.
    pub unmatched: usize,
}

/// Resolves a page set against the crawl file; `max` keeps the first pages,
/// so a cap picks the pages closest to the start page.
pub fn select_urls(conn: &Connection, set: &UrlSet, max: Option<u32>) -> Result<Selection> {
    let limit = max.map_or(-1, i64::from);
    let (extra, prefix) = match set {
        UrlSet::List { urls } => {
            let matcher = Matcher::build(conn)?;
            let mut check =
                conn.prepare(&format!("SELECT 1 FROM urls u WHERE u.id = ?1 AND {HTML_PAGES}"))?;
            let mut seen = HashSet::new();
            let mut ids = Vec::new();
            let mut unmatched = 0;
            for raw in urls {
                match matcher.lookup(raw)? {
                    Some(id) if check.exists([id])? => {
                        if seen.insert(id) {
                            ids.push(id);
                        }
                    }
                    _ => unmatched += 1,
                }
            }
            if let Ok(n) = usize::try_from(limit) {
                ids.truncate(n);
            }
            return Ok(Selection { ids, unmatched });
        }
        UrlSet::All => (String::new(), String::new()),
        UrlSet::Indexable => (format!("AND {INDEXABLE}"), String::new()),
        UrlSet::Prefix { prefix } => (String::new(), prefix.clone()),
    };
    let sql = format!(
        "SELECT u.id FROM urls u WHERE {HTML_PAGES} {extra} AND substr(u.url, 1, length(?1)) = ?1
         ORDER BY u.depth, u.id LIMIT ?2"
    );
    let ids = conn
        .prepare(&sql)?
        .query_map(params![prefix, limit], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<i64>>>()?;
    Ok(Selection { ids, unmatched: 0 })
}

/// What a run is expected to cost, shown before it starts (FR-004, SC-003).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PerfEstimate {
    /// Pages the run covers.
    pub urls: u32,
    /// Requests sent at most (answers already cached send none).
    pub requests: u32,
    pub per_minute: u32,
    pub concurrency: u32,
    /// How long the run takes, at the provider's rate.
    pub duration_secs: u32,
    /// Days of the provider's daily allowance the run needs.
    pub days_needed: u32,
    /// Share of one day's allowance the run uses, when the provider has one.
    pub share_of_allowance: Option<f64>,
}

fn clamp_u32(n: u64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

impl PerfEstimate {
    /// `seconds_per_request` is how long one request takes to answer; with
    /// `limits.concurrency` requests in flight it bounds the pace besides
    /// the per-minute rate.
    pub fn new(urls: usize, requests_per_url: usize, limits: &Limits, seconds_per_request: u64) -> Self {
        let requests = (urls as u64) * (requests_per_url as u64);
        let rate = u64::from(limits.per_minute.max(1));
        let conc = u64::from(limits.concurrency.max(1));
        let by_rate = (requests * 60).div_ceil(rate);
        let by_latency = (requests * seconds_per_request).div_ceil(conc);
        let allowance = limits.daily.map(|d| d.allowance.max(1));
        Self {
            urls: clamp_u32(urls as u64),
            requests: clamp_u32(requests),
            per_minute: limits.per_minute,
            concurrency: limits.concurrency,
            duration_secs: clamp_u32(by_rate.max(by_latency)),
            days_needed: clamp_u32(allowance.map_or(1, |a| requests.div_ceil(a).max(1))),
            share_of_allowance: allowance.map(|a| requests as f64 / a as f64),
        }
    }
}

/// One page waiting in a run's queue.
#[derive(Debug, Clone)]
pub struct Item {
    pub url_id: i64,
    pub url: String,
}

fn enqueue(conn: &Connection, run_id: i64, ids: &[i64]) -> Result<()> {
    let mut s = conn.prepare_cached(
        "INSERT INTO integration_queue (run_id, url_id, pos, state) VALUES (?1, ?2, ?3, 'pending')",
    )?;
    for (pos, id) in ids.iter().enumerate() {
        s.execute(params![run_id, id, pos as i64])?;
    }
    Ok(())
}

fn pending(conn: &Connection, run_id: i64, limit: i64) -> Result<Vec<Item>> {
    let mut s = conn.prepare_cached(
        "SELECT q.url_id, u.url FROM integration_queue q JOIN urls u ON u.id = q.url_id
         WHERE q.run_id = ?1 AND q.state = 'pending' ORDER BY q.pos LIMIT ?2",
    )?;
    let rows = s
        .query_map(params![run_id, limit], |r| {
            Ok(Item {
                url_id: r.get(0)?,
                url: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Starts a run over the pages `select` picks, or picks up `resume` where it
/// stopped (FR-005). Returns the run id and its counts so far.
pub fn begin(
    conn: &Connection,
    provider: Provider,
    label: &str,
    params_json: &serde_json::Value,
    select: impl FnOnce(&Connection) -> Result<Vec<i64>>,
    resume: Option<i64>,
) -> Result<(i64, Counts)> {
    if let Some(id) = resume {
        let p = store::run(conn, id)?.ok_or_else(|| Error::Invalid(format!("no run {id} in this crawl")))?;
        if p.provider != provider {
            return Err(Error::Invalid(format!(
                "run {id} is not a {} run",
                provider.label()
            )));
        }
        store::set_state(conn, id, RunState::Running, None, None)?;
        return Ok((
            id,
            Counts {
                requested: p.requested,
                done: p.done,
                failed: p.failed,
                matched: p.matched,
                unmatched: p.unmatched,
                usage: p.usage,
            },
        ));
    }
    let ids = select(conn)?;
    if ids.is_empty() {
        return Err(Error::Invalid("no crawled pages match the chosen pages".into()));
    }
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let made = (|| -> Result<(i64, Counts)> {
        let id = store::create_run(conn, provider, Some(label), None, params_json)?;
        enqueue(conn, id, &ids)?;
        let counts = Counts {
            requested: ids.len() as i64,
            ..Counts::default()
        };
        store::save_progress(conn, id, &counts, None)?;
        Ok((id, counts))
    })();
    match made {
        Ok(v) => {
            conn.execute_batch("COMMIT")?;
            Ok(v)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// A provider's daily allowance counted in the shared usage ledger, so the
/// app and the CLI stop at the same point (plan D5).
pub struct Allowance {
    ledger: Ledger,
    key: String,
    daily: Daily,
}

impl Allowance {
    pub fn new(ledger: Ledger, key: String, daily: Daily) -> Self {
        Self { ledger, key, daily }
    }

    fn day(&self) -> String {
        provider_day(Timestamp::now(), self.daily.tz)
    }

    pub fn used_today(&self) -> u64 {
        self.ledger.used(&self.key, &self.day())
    }

    fn exhausted(&self) -> bool {
        self.used_today() >= self.daily.allowance
    }

    fn add(&self, n: u64) {
        if n > 0 {
            // A ledger that cannot be written only loses the count.
            let _ = self.ledger.add(&self.key, &self.day(), n);
        }
    }

    fn reset(&self) -> Timestamp {
        next_reset(Timestamp::now(), self.daily.tz)
    }
}

/// The result of fetching one page: what came back, what went wrong, and
/// how many requests it took (for the retry count and the daily ledger).
pub struct Attempt<T> {
    pub value: Option<T>,
    pub error: Option<HttpError>,
    pub attempts: u32,
    /// Requests that reached the provider (not answered from the cache).
    pub requests: u64,
}

enum Stop {
    /// The daily quota is used up: pause until it resets.
    Quota(Timestamp),
    /// The provider refuses the key: every further page would fail.
    Fatal(String),
}

/// Polls every future until all are ready. The page workers of one run
/// share the crawl file's connection, so they run on one thread and never
/// need to be `Send`.
async fn join_all<F: std::future::Future<Output = ()>>(futures: Vec<F>) {
    let mut futures: Vec<Option<std::pin::Pin<Box<F>>>> =
        futures.into_iter().map(|f| Some(Box::pin(f))).collect();
    std::future::poll_fn(move |cx| {
        let mut waiting = false;
        for slot in futures.iter_mut() {
            if let Some(f) = slot {
                if f.as_mut().poll(cx).is_ready() {
                    *slot = None;
                } else {
                    waiting = true;
                }
            }
        }
        if waiting { Poll::Pending } else { Poll::Ready(()) }
    })
    .await;
}

fn short(s: &str) -> String {
    s.chars().take(300).collect()
}

/// Works through a run's queue.
pub struct Driver<'a> {
    pub ctx: &'a Ctx,
    pub conn: &'a Connection,
    pub run_id: i64,
    pub control: &'a Control,
    pub allowance: Option<&'a Allowance>,
}

impl Driver<'_> {
    /// Fetches every waiting page with up to `ctx.limits.concurrency` in
    /// flight. `fetch` does the requests for one page (taking the rate
    /// limit itself, per request); `save` writes what came back and says
    /// whether the page has any data.
    pub async fn run<T, F, Fut, S>(
        &self,
        counts: Counts,
        on_progress: &mut dyn FnMut(&RunProgress),
        fetch: F,
        save: S,
    ) -> Result<RunProgress>
    where
        F: Fn(Item) -> Fut,
        Fut: std::future::Future<Output = Attempt<T>>,
        S: Fn(&Connection, i64, &Item, T) -> Result<bool>,
    {
        let counts = RefCell::new(counts);
        let on_progress = RefCell::new(on_progress);
        let report = || -> Result<()> {
            let p = store::run(self.conn, self.run_id)?.expect("run exists");
            let mut f = on_progress.borrow_mut();
            (*f)(&p);
            Ok(())
        };
        report()?;
        loop {
            if self.control.cancelled() {
                store::set_state(self.conn, self.run_id, RunState::Cancelled, None, None)?;
                report()?;
                return self.progress();
            }
            if self.control.paused() {
                store::set_state(self.conn, self.run_id, RunState::PausedUser, None, None)?;
                report()?;
                return self.progress();
            }
            let chunk = pending(self.conn, self.run_id, CHUNK)?;
            if chunk.is_empty() {
                break;
            }
            let workers = (self.ctx.limits.concurrency.max(1) as usize).min(chunk.len());
            let queue = RefCell::new(VecDeque::from(chunk));
            let stop: RefCell<Option<Stop>> = RefCell::new(None);
            let failure: RefCell<Option<Error>> = RefCell::new(None);
            let (queue_ref, stop_ref, failure_ref) = (&queue, &stop, &failure);
            let (counts_ref, report_ref, fetch_ref, save_ref) = (&counts, &report, &fetch, &save);
            let worker = || async move {
                loop {
                    if stop_ref.borrow().is_some() || self.control.cancelled() || self.control.paused() {
                        break;
                    }
                    if let Some(a) = self.allowance
                        && a.exhausted()
                    {
                        stop_ref.borrow_mut().get_or_insert(Stop::Quota(a.reset()));
                        break;
                    }
                    let Some(item) = queue_ref.borrow_mut().pop_front() else {
                        break;
                    };
                    let attempt = fetch_ref(item.clone()).await;
                    if let Some(a) = self.allowance {
                        a.add(attempt.requests);
                    }
                    match self.settle(&item, attempt, counts_ref, save_ref) {
                        Ok(Some(s)) => {
                            stop_ref.borrow_mut().get_or_insert(s);
                        }
                        Ok(None) => {}
                        Err(e) => {
                            failure_ref.borrow_mut().get_or_insert(e);
                            stop_ref.borrow_mut().get_or_insert(Stop::Fatal(String::new()));
                            break;
                        }
                    }
                    if let Err(e) = report_ref() {
                        failure_ref.borrow_mut().get_or_insert(e);
                        stop_ref.borrow_mut().get_or_insert(Stop::Fatal(String::new()));
                        break;
                    }
                }
            };
            join_all((0..workers).map(|_| worker()).collect()).await;
            if let Some(e) = failure.into_inner() {
                return Err(e);
            }
            match stop.into_inner() {
                Some(Stop::Quota(at)) => {
                    store::set_state(
                        self.conn,
                        self.run_id,
                        RunState::PausedQuota,
                        Some("The provider's daily allowance is used up."),
                        Some(at),
                    )?;
                    report()?;
                    return self.progress();
                }
                Some(Stop::Fatal(message)) => {
                    store::set_state(self.conn, self.run_id, RunState::Failed, Some(&message), None)?;
                    report()?;
                    return self.progress();
                }
                None => {}
            }
        }
        let failed = counts.borrow().failed;
        let message =
            (failed > 0).then(|| format!("{failed} pages could not be fetched; see the pages' status."));
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        store::save_progress(self.conn, self.run_id, &counts.borrow(), None)?;
        store::set_state(
            self.conn,
            self.run_id,
            RunState::Finished,
            message.as_deref(),
            None,
        )?;
        self.conn.execute_batch("COMMIT")?;
        report()?;
        self.progress()
    }

    fn progress(&self) -> Result<RunProgress> {
        Ok(store::run(self.conn, self.run_id)?.expect("run exists"))
    }

    /// Writes one page's outcome in one transaction. Returns a reason to stop
    /// the whole run, if the outcome gives one.
    fn settle<T, S>(
        &self,
        item: &Item,
        attempt: Attempt<T>,
        counts: &RefCell<Counts>,
        save: &S,
    ) -> Result<Option<Stop>>
    where
        S: Fn(&Connection, i64, &Item, T) -> Result<bool>,
    {
        let provider = self.ctx.provider.id();
        let mut stop = None;
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let written = (|| -> Result<()> {
            let mut has = false;
            if let Some(v) = attempt.value {
                has = save(self.conn, self.run_id, item, v)?;
            }
            let mut c = counts.borrow_mut();
            let queue_state = match &attempt.error {
                None => {
                    c.done += 1;
                    if has {
                        self.conn.execute(
                            "DELETE FROM integration_status WHERE url_id = ?1 AND provider = ?2",
                            params![item.url_id, provider],
                        )?;
                    } else {
                        self.status(item, "no_data", None, attempt.attempts, None)?;
                    }
                    Some("done")
                }
                Some(HttpError::RateLimited {
                    quota: true,
                    retry_after,
                    ..
                }) => {
                    let now = Timestamp::now();
                    stop = Some(Stop::Quota(match retry_after {
                        Some(d) => now + jiff::SignedDuration::try_from(*d).unwrap_or_default(),
                        None => next_reset(now, GOOGLE_TZ),
                    }));
                    None
                }
                Some(e @ (HttpError::Unauthorized { .. } | HttpError::Forbidden { .. })) => {
                    stop = Some(Stop::Fatal(e.to_string()));
                    None
                }
                Some(e) => {
                    c.failed += 1;
                    let detail = e.to_string();
                    self.status(
                        item,
                        "provider_error",
                        Some(e.class()),
                        attempt.attempts,
                        Some(&detail),
                    )?;
                    Some("failed")
                }
            };
            if let Some(state) = queue_state {
                self.conn.execute(
                    "UPDATE integration_queue SET state = ?3 WHERE run_id = ?1 AND url_id = ?2",
                    params![self.run_id, item.url_id, state],
                )?;
            }
            c.usage = Some(c.usage.unwrap_or(0) + attempt.requests as i64);
            store::save_progress(self.conn, self.run_id, &c, None)?;
            Ok(())
        })();
        match written {
            Ok(()) => self.conn.execute_batch("COMMIT")?,
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                return Err(e);
            }
        }
        Ok(stop)
    }

    /// Why a page has no value (FR-007).
    fn status(
        &self,
        item: &Item,
        reason: &str,
        class: Option<&str>,
        attempts: u32,
        detail: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO integration_status (url_id, provider, run_id, reason, error_class, attempts, detail)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(url_id, provider) DO UPDATE SET run_id = excluded.run_id, reason = excluded.reason,
               error_class = excluded.error_class, attempts = excluded.attempts, detail = excluded.detail",
            params![
                item.url_id,
                self.ctx.provider.id(),
                self.run_id,
                reason,
                class,
                attempts,
                detail.map(short)
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT NOT NULL UNIQUE, internal INTEGER NOT NULL,
               depth INTEGER NOT NULL, state TEXT NOT NULL, error_class TEXT, status INTEGER,
               content_type TEXT, noindex INTEGER, canonical TEXT);",
        )
        .unwrap();
        for (url, internal, depth, status, ct, noindex, canonical) in [
            ("https://a.test/", 1, 0, 200, "text/html; charset=utf-8", 0, None),
            ("https://a.test/blog/a", 1, 1, 200, "text/html", 0, None),
            ("https://a.test/blog/b", 1, 1, 200, "text/html", 1, None),
            (
                "https://a.test/blog/c",
                1,
                2,
                200,
                "text/html",
                0,
                Some("https://a.test/blog/a"),
            ),
            ("https://a.test/missing", 1, 1, 404, "text/html", 0, None),
            ("https://a.test/logo.png", 1, 1, 200, "image/png", 0, None),
            ("https://x.test/", 0, 1, 200, "text/html", 0, None),
        ] {
            c.execute(
                "INSERT INTO urls (url, internal, depth, state, status, content_type, noindex, canonical)
                 VALUES (?1, ?2, ?3, 'crawled', ?4, ?5, ?6, ?7)",
                params![url, internal, depth, status, ct, noindex, canonical],
            )
            .unwrap();
        }
        c
    }

    fn urls(c: &Connection, ids: &[i64]) -> Vec<String> {
        ids.iter()
            .map(|id| {
                c.query_row("SELECT url FROM urls WHERE id = ?1", [id], |r| r.get(0))
                    .unwrap()
            })
            .collect()
    }

    #[test]
    fn page_sets_pick_internal_html_pages_shallowest_first() {
        let c = conn();
        let all = select_urls(&c, &UrlSet::All, None).unwrap();
        assert_eq!(
            urls(&c, &all.ids),
            [
                "https://a.test/",
                "https://a.test/blog/a",
                "https://a.test/blog/b",
                "https://a.test/blog/c"
            ]
        );
        let indexable = select_urls(&c, &UrlSet::Indexable, None).unwrap();
        assert_eq!(
            urls(&c, &indexable.ids),
            ["https://a.test/", "https://a.test/blog/a"]
        );
        let blog = select_urls(
            &c,
            &UrlSet::Prefix {
                prefix: "https://a.test/blog/".into(),
            },
            Some(2),
        )
        .unwrap();
        assert_eq!(
            urls(&c, &blog.ids),
            ["https://a.test/blog/a", "https://a.test/blog/b"]
        );
    }

    #[test]
    fn a_list_matches_crawled_pages_and_counts_the_rest() {
        let c = conn();
        let list = UrlSet::List {
            urls: vec![
                "https://a.test/blog/a/".into(),
                "https://a.test/blog/a".into(),
                "https://a.test/logo.png".into(),
                "https://elsewhere.test/".into(),
            ],
        };
        let s = select_urls(&c, &list, None).unwrap();
        assert_eq!(urls(&c, &s.ids), ["https://a.test/blog/a"]);
        assert_eq!(s.unmatched, 2);
    }

    #[test]
    fn estimate_follows_the_slower_of_rate_and_latency() {
        let limits = Limits::defaults(Provider::FieldData);
        // 1,000 pages on two devices at 140 a minute: under SC-003's 15 minutes.
        let e = PerfEstimate::new(1000, 2, &limits, 1);
        assert_eq!(e.requests, 2000);
        assert!(e.duration_secs <= 15 * 60, "{}", e.duration_secs);
        assert_eq!(e.days_needed, 1);
        assert_eq!(e.share_of_allowance, None);

        // Lab tests are limited by how long Lighthouse takes, not by the rate.
        let lab = Limits::defaults(Provider::LabTests);
        let e = PerfEstimate::new(1000, 2, &lab, 25);
        assert_eq!(e.duration_secs, 2000 * 25 / 6 + 1);
        assert_eq!(e.share_of_allowance, Some(2000.0 / 25_000.0));
        let e = PerfEstimate::new(30_000, 1, &lab, 25);
        assert_eq!(e.days_needed, 2);
    }

    #[tokio::test]
    async fn join_all_runs_every_future() {
        let done = std::cell::Cell::new(0);
        join_all((0..5).map(|_| async { done.set(done.get() + 1) }).collect()).await;
        assert_eq!(done.get(), 5);
    }
}
