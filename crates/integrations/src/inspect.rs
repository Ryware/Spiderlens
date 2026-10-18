//! URL Inspection (spec 024 US2, plan D10): Google's own view of a page.
//!
//! The user picks a set of crawled URLs (default: indexable HTML pages,
//! ordered by Search Console clicks when there are any, else by crawl
//! depth). The URLs go into the run's queue in that order and are inspected
//! one at a time with `urlInspection.index:inspect`. Google allows 2,000
//! inspections per property per day, so the run counts them in the usage
//! ledger, stops with a "resumes after" time when the allowance is used up
//! and continues the next day with the URLs still pending (FR-022).

use jiff::{SignedDuration, Timestamp};
use reqwest::Method;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use spiderlens_engine::crawl::Update;
use ts_rs::TS;

use crate::gcall::{call, in_tx};
use crate::http::HttpError;
use crate::limits::{Daily, GOOGLE_TZ, Ledger, next_reset, provider_day};
use crate::matching::primary_key;
use crate::oauth::Session;
use crate::run::{Control, Ctx};
use crate::store::{self, Counts, RunProgress, RunState};
use crate::{Error, Provider, Result, cache};

#[derive(Debug, Clone)]
pub struct InspectApi {
    pub base: String,
}

impl Default for InspectApi {
    fn default() -> Self {
        Self {
            base: "https://searchconsole.googleapis.com/v1".into(),
        }
    }
}

/// What an inspection run asks for (US2.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct InspectRequest {
    /// The Search Console property the URLs belong to.
    pub property: String,
    /// `indexable` (the default set), `all` crawled internal URLs, or any
    /// other text: indexable pages whose address contains it.
    pub urls: String,
    /// At most this many URLs, in queue order.
    #[serde(default)]
    pub max: Option<u32>,
    /// Skip cached answers.
    #[serde(default)]
    pub fresh: bool,
}

impl InspectRequest {
    pub fn indexable(property: impl Into<String>) -> Self {
        Self {
            property: property.into(),
            urls: "indexable".into(),
            max: None,
            fresh: false,
        }
    }
}

/// What the confirmation shows before a run starts: the URL count against
/// today's remaining allowance (US2.1, US2.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct InspectPlan {
    #[ts(type = "number")]
    pub selected: i64,
    #[ts(type = "number")]
    pub allowance: u64,
    #[ts(type = "number")]
    pub used_today: u64,
    #[ts(type = "number")]
    pub remaining_today: u64,
    /// RFC 3339: when the allowance resets (midnight Pacific).
    pub resets_at: String,
    /// Days the run needs at the daily allowance; more than one is warned about.
    pub days_needed: u32,
}

/// Indexable HTML pages of the site: the same rule as the Indexability
/// column, written without the results crate's SQL functions.
const INDEXABLE_HTML: &str = "u.internal = 1 AND u.state = 'crawled' AND u.error_class IS NULL \
     AND u.status BETWEEN 200 AND 299 \
     AND (lower(u.content_type) LIKE 'text/html%' OR lower(u.content_type) LIKE 'application/xhtml%') \
     AND COALESCE(u.noindex, 0) = 0 \
     AND (u.canonical IS NULL OR u.canonical = '' OR u.canonical = u.url)";

const ALL_INTERNAL: &str = "u.internal = 1 AND u.state = 'crawled' AND u.error_class IS NULL";

/// The WHERE clause for a URL set rule, and the text filter it carries.
/// `?1` in the clause is the lower-cased filter text, or NULL.
fn selection(rule: &str) -> (&'static str, Option<String>) {
    match rule.trim() {
        "all" => (ALL_INTERNAL, None),
        "" | "indexable" => (INDEXABLE_HTML, None),
        text => (INDEXABLE_HTML, Some(text.to_lowercase())),
    }
}

/// How many URLs the request selects.
pub fn count_selected(conn: &Connection, req: &InspectRequest) -> Result<i64> {
    let (clause, filter) = selection(&req.urls);
    let n: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM urls u WHERE {clause} AND (?1 IS NULL OR instr(lower(u.url), ?1) > 0)"
        ),
        [filter],
        |r| r.get(0),
    )?;
    Ok(req.max.map_or(n, |m| n.min(i64::from(m))))
}

/// Days needed for `selected` URLs when `remaining` inspections are left
/// today and each later day allows `allowance`.
pub fn days_needed(selected: u64, remaining: u64, allowance: u64) -> u32 {
    if selected <= remaining {
        return 1;
    }
    let more = (selected - remaining).div_ceil(allowance.max(1));
    u32::try_from(more + 1).unwrap_or(u32::MAX)
}

/// The count against today's allowance (US2.1, US2.4).
pub fn plan(
    conn: &Connection,
    req: &InspectRequest,
    daily: Daily,
    used_today: u64,
    now: Timestamp,
) -> Result<InspectPlan> {
    let selected = count_selected(conn, req)?;
    let remaining = daily.allowance.saturating_sub(used_today);
    Ok(InspectPlan {
        selected,
        allowance: daily.allowance,
        used_today,
        remaining_today: remaining,
        resets_at: next_reset(now, daily.tz).to_string(),
        days_needed: days_needed(selected.max(0) as u64, remaining, daily.allowance),
    })
}

/// The daily allowance in force for a context.
pub fn daily_of(ctx: &Ctx) -> Daily {
    ctx.limits.daily.unwrap_or(Daily {
        allowance: 2000,
        tz: GOOGLE_TZ,
    })
}

/// One result row, as stored in `gsc_inspection`.
#[derive(Debug, Clone, PartialEq)]
pub struct Inspected {
    pub verdict: Option<String>,
    pub coverage_state: Option<String>,
    pub indexing_state: Option<String>,
    pub robots_state: Option<String>,
    pub page_fetch_state: Option<String>,
    pub google_canonical: Option<String>,
    pub user_canonical: Option<String>,
    /// Google's canonical differs from the crawled canonical (US2.2).
    pub canonical_mismatch: Option<bool>,
    pub last_crawl: Option<i64>,
    pub crawled_as: Option<String>,
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Reads an inspection answer. `crawled_canonical` is the canonical the
/// crawl read from the page (none: the page names itself).
pub fn parse(answer: &Value, crawled_canonical: Option<&str>, page_url: &str) -> Option<Inspected> {
    let index = answer.pointer("/inspectionResult/indexStatusResult")?;
    let google_canonical = text(index, "googleCanonical");
    let canonical_mismatch = google_canonical.as_deref().map(|g| {
        let crawled = crawled_canonical.filter(|c| !c.is_empty()).unwrap_or(page_url);
        let norm = |x: &str| primary_key(x).unwrap_or_else(|| x.to_string());
        norm(g) != norm(crawled)
    });
    Some(Inspected {
        verdict: text(index, "verdict"),
        coverage_state: text(index, "coverageState"),
        indexing_state: text(index, "indexingState"),
        robots_state: text(index, "robotsTxtState"),
        page_fetch_state: text(index, "pageFetchState"),
        google_canonical,
        user_canonical: text(index, "userCanonical"),
        canonical_mismatch,
        last_crawl: text(index, "lastCrawlTime")
            .and_then(|t| t.parse::<Timestamp>().ok())
            .map(Timestamp::as_second),
        crawled_as: text(index, "crawledAs"),
    })
}

fn write_result(
    conn: &Connection,
    run_id: i64,
    url_id: i64,
    r: &Inspected,
    raw: &[u8],
    now: Timestamp,
) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO gsc_inspection (url_id, run_id, verdict, coverage_state, indexing_state,
           robots_state, page_fetch_state, google_canonical, user_canonical, canonical_mismatch,
           last_crawl, crawled_as, inspected_at, raw)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            url_id,
            run_id,
            r.verdict,
            r.coverage_state,
            r.indexing_state,
            r.robots_state,
            r.page_fetch_state,
            r.google_canonical,
            r.user_canonical,
            r.canonical_mismatch,
            r.last_crawl,
            r.crawled_as,
            now.as_second(),
            raw
        ],
    )?;
    Ok(())
}

fn record_failure(
    conn: &Connection,
    run_id: i64,
    url_id: i64,
    e: &HttpError,
    attempts: u32,
    counts: &mut Counts,
) -> Result<()> {
    counts.failed += 1;
    let reason = if matches!(e, HttpError::NotFound { .. }) {
        "no_data"
    } else {
        "provider_error"
    };
    let counts = *counts;
    in_tx(conn, || {
        conn.execute(
            "INSERT OR REPLACE INTO integration_status (url_id, provider, run_id, reason, error_class, attempts, detail)
             VALUES (?1, 'url_inspection', ?2, ?3, ?4, ?5, ?6)",
            params![url_id, run_id, reason, e.class(), attempts, e.to_string()],
        )?;
        conn.execute(
            "UPDATE integration_queue SET state = 'failed' WHERE run_id = ?1 AND url_id = ?2",
            params![run_id, url_id],
        )?;
        store::save_progress(conn, run_id, &counts, None)
    })
}

/// Stops the run at a limit with the URLs still pending kept and marked as
/// "not inspected: quota" (FR-007).
fn pause_quota(conn: &Connection, run_id: i64, counts: &Counts, message: &str, at: Timestamp) -> Result<()> {
    in_tx(conn, || {
        store::save_progress(conn, run_id, counts, None)?;
        conn.execute(
            "INSERT OR REPLACE INTO integration_status (url_id, provider, run_id, reason)
             SELECT q.url_id, 'url_inspection', ?1, 'quota' FROM integration_queue q
             WHERE q.run_id = ?1 AND q.state = 'pending'",
            [run_id],
        )?;
        store::set_state(conn, run_id, RunState::PausedQuota, Some(message), Some(at))
    })
}

fn fail_run(conn: &Connection, run_id: i64, counts: &Counts, message: &str) -> Result<()> {
    in_tx(conn, || {
        store::save_progress(conn, run_id, counts, None)?;
        store::set_state(conn, run_id, RunState::Failed, Some(message), None)
    })
}

/// Starts an inspection run, or continues `resume` with the URLs still
/// pending. `clock` is the provider's idea of now (tests move it across
/// the daily reset).
#[allow(clippy::too_many_arguments)]
pub async fn run_inspection(
    ctx: &Ctx,
    session: &Session,
    api: &InspectApi,
    update: &Update,
    ledger: &Ledger,
    account_label: &str,
    req: &InspectRequest,
    resume: Option<i64>,
    control: &Control,
    clock: &dyn Fn() -> Timestamp,
    on_progress: &mut dyn FnMut(&RunProgress),
) -> Result<RunProgress> {
    let conn = update.conn();
    let daily = daily_of(ctx);
    let ledger_key = Ledger::key(Provider::UrlInspection, account_label, &req.property);
    let params_json = serde_json::to_value(req).expect("serializes");
    let (run_id, mut counts) = match resume {
        Some(id) => {
            let p =
                store::run(conn, id)?.ok_or_else(|| Error::Invalid(format!("no run {id} in this crawl")))?;
            store::set_state(conn, id, RunState::Running, None, None)?;
            (
                id,
                Counts {
                    requested: p.requested,
                    done: p.done,
                    failed: p.failed,
                    matched: None,
                    unmatched: None,
                    usage: p.usage,
                },
            )
        }
        None => in_tx(conn, || {
            conn.execute(
                "DELETE FROM integration_status WHERE provider = 'url_inspection'",
                [],
            )?;
            let id = store::create_run(
                conn,
                Provider::UrlInspection,
                Some(account_label),
                Some(&req.property),
                &params_json,
            )?;
            let (clause, filter) = selection(&req.urls);
            conn.execute(
                &format!(
                    "INSERT INTO integration_queue (run_id, url_id, pos, state)
                     SELECT ?2, u.id, ROW_NUMBER() OVER (ORDER BY COALESCE(g.clicks, 0) DESC, u.depth, u.id) AS pos,
                            'pending'
                     FROM urls u LEFT JOIN gsc_pages g ON g.url_id = u.id
                     WHERE {clause} AND (?1 IS NULL OR instr(lower(u.url), ?1) > 0)
                     ORDER BY pos LIMIT ?3"
                ),
                params![filter, id, req.max.map_or(-1, i64::from)],
            )?;
            let requested: i64 = conn.query_row(
                "SELECT COUNT(*) FROM integration_queue WHERE run_id = ?1",
                [id],
                |r| r.get(0),
            )?;
            let counts = Counts {
                requested,
                usage: Some(0),
                ..Counts::default()
            };
            store::save_progress(conn, id, &counts, None)?;
            Ok((id, counts))
        })?,
    };
    let report = |on_progress: &mut dyn FnMut(&RunProgress)| -> Result<RunProgress> {
        let p = store::run(conn, run_id)?.expect("run exists");
        on_progress(&p);
        Ok(p)
    };
    report(on_progress)?;
    let endpoint = format!("{}/urlInspection/index:inspect", api.base);
    let mut streak = 0u32;
    loop {
        if control.cancelled() {
            store::set_state(conn, run_id, RunState::Cancelled, None, None)?;
            return report(on_progress);
        }
        if control.paused() {
            store::set_state(conn, run_id, RunState::PausedUser, None, None)?;
            return report(on_progress);
        }
        let next: Option<(i64, String, Option<String>)> = conn
            .query_row(
                "SELECT q.url_id, u.url, u.canonical FROM integration_queue q JOIN urls u ON u.id = q.url_id
                 WHERE q.run_id = ?1 AND q.state = 'pending' ORDER BY q.pos LIMIT 1",
                [run_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((url_id, page_url, crawled_canonical)) = next else {
            break;
        };
        let cache_key = cache::key(&["url_inspection", account_label, &req.property, &page_url]);
        let answer = match ctx.cached(&cache_key) {
            Some(v) => v,
            None => {
                let now = clock();
                let day = provider_day(now, daily.tz);
                let used = ledger.used(&ledger_key, &day);
                if used >= daily.allowance {
                    let at = next_reset(now, daily.tz);
                    let left = counts.requested - counts.done - counts.failed;
                    let message = format!(
                        "Today's URL Inspection allowance for this property is used up ({used} of {}). {left} URLs are left; the run continues after {at}.",
                        daily.allowance
                    );
                    pause_quota(conn, run_id, &counts, &message, at)?;
                    return report(on_progress);
                }
                let body = json!({
                    "inspectionUrl": page_url,
                    "siteUrl": req.property,
                    "languageCode": "en-US",
                });
                let (r, attempts) = call(
                    ctx,
                    session,
                    "URL Inspection",
                    Method::POST,
                    &endpoint,
                    Some(&body),
                )
                .await;
                match r {
                    Ok(v) => {
                        ledger.add(&ledger_key, &day, 1)?;
                        counts.usage = Some(counts.usage.unwrap_or(0) + 1);
                        ctx.remember(&cache_key, &v);
                        v
                    }
                    Err(HttpError::RateLimited {
                        quota,
                        retry_after,
                        message: why,
                        ..
                    }) => {
                        let at = if quota {
                            // Google says the day's allowance is gone, whatever our own
                            // count was (another computer may have used it).
                            let used_now = ledger.used(&ledger_key, &day);
                            if used_now < daily.allowance {
                                ledger.add(&ledger_key, &day, daily.allowance - used_now)?;
                            }
                            next_reset(now, daily.tz)
                        } else {
                            now + SignedDuration::from_secs(retry_after.map_or(60, |d| d.as_secs() as i64))
                        };
                        let message =
                            format!("Google asked SpiderLens to stop: {why}. The run continues after {at}.");
                        pause_quota(conn, run_id, &counts, &message, at)?;
                        return report(on_progress);
                    }
                    Err(e @ (HttpError::Unauthorized { .. } | HttpError::Forbidden { .. })) => {
                        // No access to the property: every URL would fail the same way.
                        fail_run(conn, run_id, &counts, &e.to_string())?;
                        return report(on_progress);
                    }
                    Err(e) => {
                        streak += 1;
                        record_failure(conn, run_id, url_id, &e, attempts, &mut counts)?;
                        if streak >= 5 && e.is_transient() {
                            fail_run(conn, run_id, &counts, &e.to_string())?;
                            return report(on_progress);
                        }
                        report(on_progress)?;
                        continue;
                    }
                }
            }
        };
        streak = 0;
        let Some(result) = parse(&answer, crawled_canonical.as_deref(), &page_url) else {
            let e = HttpError::NotFound {
                provider: "URL Inspection".into(),
            };
            record_failure(conn, run_id, url_id, &e, 1, &mut counts)?;
            report(on_progress)?;
            continue;
        };
        let raw = zstd::encode_all(answer.to_string().as_bytes(), 3)?;
        counts.done += 1;
        let counts_now = counts;
        in_tx(conn, || {
            write_result(conn, run_id, url_id, &result, &raw, clock())?;
            conn.execute(
                "DELETE FROM integration_status WHERE url_id = ?1 AND provider = 'url_inspection'",
                [url_id],
            )?;
            conn.execute(
                "UPDATE integration_queue SET state = 'done' WHERE run_id = ?1 AND url_id = ?2",
                params![run_id, url_id],
            )?;
            store::save_progress(conn, run_id, &counts_now, None)
        })?;
        report(on_progress)?;
    }
    in_tx(conn, || {
        store::save_progress(conn, run_id, &counts, None)?;
        store::set_state(conn, run_id, RunState::Finished, None, None)
    })?;
    report(on_progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_needed_counts_today_then_full_days() {
        assert_eq!(days_needed(100, 2000, 2000), 1);
        assert_eq!(days_needed(2000, 2000, 2000), 1);
        assert_eq!(days_needed(2001, 2000, 2000), 2);
        assert_eq!(days_needed(100, 0, 2000), 2);
        assert_eq!(days_needed(6000, 500, 2000), 4);
    }

    #[test]
    fn rules_pick_the_url_set() {
        assert_eq!(selection("indexable").1, None);
        assert_eq!(selection("all").0, ALL_INTERNAL);
        assert_eq!(selection(" /Blog/ ").1.as_deref(), Some("/blog/"));
    }

    fn answer(google: &str, user: &str) -> Value {
        json!({"inspectionResult": {"indexStatusResult": {
            "verdict": "PASS",
            "coverageState": "Submitted and indexed",
            "robotsTxtState": "ALLOWED",
            "indexingState": "INDEXING_ALLOWED",
            "pageFetchState": "SUCCESSFUL",
            "googleCanonical": google,
            "userCanonical": user,
            "lastCrawlTime": "2026-10-12T08:30:00Z",
            "crawledAs": "MOBILE"
        }}})
    }

    #[test]
    fn canonical_mismatch_compares_with_the_crawled_canonical() {
        let page = "https://a.test/p";
        let same = parse(&answer("https://a.test/p", "https://a.test/p"), None, page).unwrap();
        assert_eq!(same.canonical_mismatch, Some(false));
        assert_eq!(same.verdict.as_deref(), Some("PASS"));
        assert_eq!(same.crawled_as.as_deref(), Some("MOBILE"));
        assert_eq!(same.last_crawl, Some(1_791_793_800));
        let other = parse(&answer("https://a.test/other", ""), None, page).unwrap();
        assert_eq!(other.canonical_mismatch, Some(true));
        assert_eq!(other.user_canonical, None);
        // The crawl read a canonical to the other page, and Google agrees with it.
        let agrees = parse(
            &answer("https://a.test/other", ""),
            Some("https://a.test/other"),
            page,
        )
        .unwrap();
        assert_eq!(agrees.canonical_mismatch, Some(false));
        let neutral = json!({"inspectionResult": {"indexStatusResult": {"verdict": "NEUTRAL"}}});
        let none = parse(&neutral, None, page).unwrap();
        assert_eq!(none.canonical_mismatch, None);
        assert!(parse(&json!({}), None, page).is_none());
    }
}
