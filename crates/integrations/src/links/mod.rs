//! Link metrics from Ahrefs, Majestic and Moz (spec 024 US5, plan D10).
//!
//! These providers charge per unit or row, so a run is shown as an estimate
//! in the provider's own units first (FR-029), is re-estimated after the
//! first paid batch from the units the provider reports, and never goes past
//! the cap the user confirmed (SC-006). Answers are cached for seven days
//! (FR-008), so repeating a run costs nothing.
//!
//! Pages are queued in `integration_queue` and fetched in batches of the
//! provider's maximum; each batch is written in one transaction together
//! with the run's counters, so a stopped run resumes from the next batch.

pub mod ahrefs;
pub mod majestic;
pub mod moz;

use jiff::Timestamp;
use reqwest::header::HeaderMap;
use rusqlite::{Connection, params};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use spiderlens_engine::crawl::Update;
use ts_rs::TS;

use crate::http::HttpError;
use crate::limits::with_retry;
use crate::run::{Control, Ctx};
use crate::store::{self, Counts, RunProgress, RunState};
use crate::{Error, Provider, Result, cache};

/// Pages in the first paid batch of a run.
const PROBE_PAGES: usize = 25;

/// Where each provider's API lives; tests point these at local fakes.
#[derive(Debug, Clone)]
pub struct Apis {
    pub ahrefs: String,
    pub majestic: String,
    pub moz: String,
}

impl Default for Apis {
    fn default() -> Self {
        Self {
            ahrefs: "https://api.ahrefs.com/v3".into(),
            majestic: "https://api.majestic.com/api/json".into(),
            moz: "https://lsapi.seomoz.com/v2".into(),
        }
    }
}

/// One metric a provider can return: our id, the label and the provider's
/// own field name.
#[derive(Debug, Clone, Copy)]
pub struct MetricDef {
    pub id: &'static str,
    pub label: &'static str,
    pub field: &'static str,
}

/// A metric as the run panel lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MetricInfo {
    pub id: String,
    pub label: String,
}

/// Which crawled pages to look up.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, rename = "LinkUrlSet")]
pub enum UrlSet {
    /// Internal HTML pages that answered 2xx and are not noindex or
    /// canonicalised elsewhere (the default, US5.1).
    #[default]
    Indexable,
    /// Every internal HTML page.
    All,
}

/// What a link-metrics run asks for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LinkRequest {
    pub provider: Provider,
    #[serde(default)]
    pub url_set: UrlSet,
    /// Metric ids; empty means all of the provider's metrics.
    #[serde(default)]
    pub metrics: Vec<String>,
    /// The most units or rows this run may use. Empty: the estimate plus 10%.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub cap: Option<u64>,
    /// Ignore cached answers.
    #[serde(default)]
    pub refresh: bool,
}

/// The cost shown before a run, in the provider's own units (FR-029).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LinkEstimate {
    pub provider: Provider,
    /// `API units`, `units` or `rows`.
    pub unit: String,
    #[ts(type = "number")]
    pub urls: u64,
    /// Pages whose answers are cached and cost nothing.
    #[ts(type = "number")]
    pub cached: u64,
    #[ts(type = "number")]
    pub requests: u64,
    /// For the pages that are not cached.
    #[ts(type = "number")]
    pub units: u64,
    /// The estimate plus 10%.
    #[ts(type = "number")]
    pub default_cap: u64,
    /// The cap that will apply: the request's, or the default.
    #[ts(type = "number")]
    pub cap: u64,
}

pub fn catalogue(p: Provider) -> &'static [MetricDef] {
    match p {
        Provider::Ahrefs => ahrefs::METRICS,
        Provider::Majestic => majestic::METRICS,
        Provider::Moz => moz::METRICS,
        _ => &[],
    }
}

pub fn metric_infos(p: Provider) -> Vec<MetricInfo> {
    catalogue(p)
        .iter()
        .map(|d| MetricInfo {
            id: d.id.into(),
            label: d.label.into(),
        })
        .collect()
}

/// The metrics a request names, or all of the provider's when it names none.
pub fn resolve_metrics(p: Provider, wanted: &[String]) -> Result<Vec<MetricDef>> {
    let all = catalogue(p);
    if all.is_empty() {
        return Err(Error::Invalid(format!(
            "{} is not a link-data provider",
            p.label()
        )));
    }
    if wanted.is_empty() {
        return Ok(all.to_vec());
    }
    let mut out: Vec<MetricDef> = Vec::new();
    for w in wanted {
        let d = all.iter().find(|d| d.id == w).ok_or_else(|| {
            let ids: Vec<&str> = all.iter().map(|d| d.id).collect();
            Error::Invalid(format!(
                "{} has no metric {w}; choose from {}",
                p.label(),
                ids.join(", ")
            ))
        })?;
        if !out.iter().any(|o| o.id == d.id) {
            out.push(*d);
        }
    }
    Ok(out)
}

fn metric_ids(defs: &[MetricDef]) -> String {
    let mut ids: Vec<&str> = defs.iter().map(|d| d.id).collect();
    ids.sort_unstable();
    ids.join(",")
}

/// Targets per request, the provider's documented maximum (one constant per
/// module, plan D10).
pub fn batch_size(p: Provider) -> usize {
    match p {
        Provider::Ahrefs => ahrefs::BATCH,
        Provider::Majestic => majestic::BATCH,
        Provider::Moz => moz::BATCH,
        _ => 1,
    }
}

/// What the provider calls its cost: Ahrefs API units, Majestic units, Moz rows.
pub fn unit_name(p: Provider) -> &'static str {
    match p {
        Provider::Ahrefs => "API units",
        Provider::Moz => "rows",
        _ => "units",
    }
}

/// The estimated cost of one request for `targets` pages and `fields` metrics.
pub fn batch_cost(p: Provider, targets: u64, fields: u64) -> u64 {
    match p {
        Provider::Ahrefs => (targets * fields).max(ahrefs::MIN_REQUEST_UNITS),
        _ => targets,
    }
}

/// Requests and units for `uncached` pages: a small first batch, then full
/// ones, as the run sends them.
pub fn estimate_units(p: Provider, uncached: u64, fields: u64) -> (u64, u64) {
    let size = batch_size(p) as u64;
    let first = uncached.min(size.min(PROBE_PAGES as u64));
    let left = uncached - first;
    let full = left / size;
    let rest = left % size;
    let requests = u64::from(first > 0) + full + u64::from(rest > 0);
    let mut units = full * batch_cost(p, size, fields);
    if first > 0 {
        units += batch_cost(p, first, fields);
    }
    if rest > 0 {
        units += batch_cost(p, rest, fields);
    }
    (requests, units)
}

fn cache_key(p: Provider, label: &str, url: &str, ids: &str) -> [u8; 16] {
    cache::key(&["link_metrics", p.id(), label, url, ids])
}

/// A cached answer: `{"m": {metric: number}, "t": fetched-at seconds}`.
fn cached_answer(v: &Value) -> Option<(Map<String, Value>, i64)> {
    Some((v.get("m")?.as_object()?.clone(), v.get("t")?.as_i64()?))
}

/// The crawled pages a run looks up, in crawl order (shallow first).
pub fn target_urls(conn: &Connection, set: UrlSet) -> Result<Vec<(i64, String)>> {
    let sql = match set {
        UrlSet::Indexable => {
            "SELECT u.id, u.url FROM urls u JOIN pages p ON p.url_id = u.id
             WHERE u.internal = 1 AND u.status BETWEEN 200 AND 299 AND COALESCE(u.noindex, 0) = 0
               AND (u.canonical IS NULL OR u.canonical = '' OR u.canonical = u.url)
             ORDER BY u.depth, u.id"
        }
        UrlSet::All => {
            "SELECT u.id, u.url FROM urls u JOIN pages p ON p.url_id = u.id
             WHERE u.internal = 1 ORDER BY u.depth, u.id"
        }
    };
    let mut s = conn.prepare(sql)?;
    let rows = s
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<(i64, String)>>>()?;
    Ok(rows)
}

fn estimate_for(
    ctx: &Ctx,
    label: &str,
    req: &LinkRequest,
    defs: &[MetricDef],
    targets: &[(i64, String)],
) -> LinkEstimate {
    let p = req.provider;
    let ids = metric_ids(defs);
    let cached = targets
        .iter()
        .filter(|(_, u)| {
            ctx.cached(&cache_key(p, label, u, &ids))
                .is_some_and(|v| cached_answer(&v).is_some())
        })
        .count() as u64;
    let urls = targets.len() as u64;
    let (requests, units) = estimate_units(p, urls - cached, defs.len() as u64);
    let default_cap = units + units.div_ceil(10);
    LinkEstimate {
        provider: p,
        unit: unit_name(p).into(),
        urls,
        cached,
        requests,
        units,
        default_cap,
        cap: req.cap.unwrap_or(default_cap),
    }
}

/// The cost of a run, before anything is sent (FR-029). Reads the crawl file
/// and the cache only.
pub fn estimate(conn: &Connection, ctx: &Ctx, label: &str, req: &LinkRequest) -> Result<LinkEstimate> {
    let defs = resolve_metrics(req.provider, &req.metrics)?;
    let targets = target_urls(conn, req.url_set)?;
    Ok(estimate_for(ctx, label, req, &defs, &targets))
}

/// What one request returned: a row per requested page, in order, and the
/// units the provider says it used.
pub struct BatchOut {
    pub rows: Vec<Option<Map<String, Value>>>,
    pub units: Option<u64>,
}

pub(crate) fn header_u64(h: &HeaderMap, name: &str) -> Option<u64> {
    h.get(name)?.to_str().ok()?.trim().parse().ok()
}

/// The numbers of `row` for the wanted metrics, by our ids.
pub(crate) fn pick(row: &Value, defs: &[MetricDef]) -> Map<String, Value> {
    defs.iter()
        .filter_map(|d| {
            row.get(d.field)
                .filter(|v| v.is_number())
                .map(|v| (d.id.to_string(), v.clone()))
        })
        .collect()
}

async fn fetch_batch(
    ctx: &Ctx,
    p: Provider,
    secret: &SecretString,
    apis: &Apis,
    urls: &[&str],
    defs: &[MetricDef],
) -> std::result::Result<BatchOut, HttpError> {
    match p {
        Provider::Ahrefs => ahrefs::fetch(ctx, secret, &apis.ahrefs, urls, defs).await,
        Provider::Majestic => majestic::fetch(ctx, secret, &apis.majestic, urls, defs).await,
        Provider::Moz => moz::fetch(ctx, secret, &apis.moz, urls, defs).await,
        other => Err(HttpError::Client {
            provider: other.label().into(),
            status: 0,
            message: "not a link-data provider".into(),
        }),
    }
}

fn report(conn: &Connection, run_id: i64, on_progress: &mut dyn FnMut(&RunProgress)) -> Result<RunProgress> {
    let p = store::run(conn, run_id)?.expect("run exists");
    on_progress(&p);
    Ok(p)
}

fn next_batch(conn: &Connection, run_id: i64, n: usize) -> Result<Vec<(i64, String)>> {
    let mut s = conn.prepare_cached(
        "SELECT q.url_id, u.url FROM integration_queue q JOIN urls u ON u.id = q.url_id
         WHERE q.run_id = ?1 AND q.state = 'pending' ORDER BY q.pos LIMIT ?2",
    )?;
    let rows = s
        .query_map(params![run_id, n as i64], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<(i64, String)>>>()?;
    Ok(rows)
}

fn pending(conn: &Connection, run_id: i64) -> Result<u64> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM integration_queue WHERE run_id = ?1 AND state = 'pending'",
        [run_id],
        |r| r.get(0),
    )?;
    Ok(n.max(0) as u64)
}

/// Why a run ended before every page was fetched.
enum Stop {
    /// The next pages would pass the cap.
    Cap,
    /// The first paid batch showed the whole run costs more than confirmed;
    /// holds the new estimate.
    Reestimate(u64),
}

fn stop_message(why: &Stop, cap: u64, unit: &str, counts: &Counts) -> String {
    match why {
        Stop::Cap => format!(
            "Stopped before passing the cap of {cap} {unit}: {} of {} pages fetched. \
             Run again with a higher cap; answers already fetched are reused at no cost.",
            counts.done, counts.requested
        ),
        Stop::Reestimate(total) => format!(
            "This run would use about {total} {unit}, more than the {cap} you confirmed \
             ({} of {} pages fetched so far). Run again with a higher cap to continue; \
             answers already fetched are reused at no cost.",
            counts.done, counts.requested
        ),
    }
}

/// What `n` pages cost: at the rate the provider showed, else by the estimate's formula.
fn price(p: Provider, rate: Option<f64>, fields: u64, n: usize) -> u64 {
    match rate {
        Some(r) => (r * n as f64).ceil() as u64,
        None => batch_cost(p, n as u64, fields),
    }
}

/// Finishes a run early because the cost would pass the confirmed cap. The
/// pages fetched so far stay; running again reuses their cached answers.
fn finish_early(
    conn: &Connection,
    run_id: i64,
    counts: &Counts,
    message: &str,
    on_progress: &mut dyn FnMut(&RunProgress),
) -> Result<RunProgress> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    store::save_progress(conn, run_id, counts, None)?;
    store::set_state(conn, run_id, RunState::Finished, Some(message), None)?;
    conn.execute_batch("COMMIT")?;
    report(conn, run_id, on_progress)
}

/// Starts a link-metrics run, or resumes `resume` from its queue.
#[allow(clippy::too_many_arguments)]
pub async fn run_links(
    ctx: &Ctx,
    secret: &SecretString,
    apis: &Apis,
    update: &Update,
    label: &str,
    req: &LinkRequest,
    resume: Option<i64>,
    control: &Control,
    on_progress: &mut dyn FnMut(&RunProgress),
) -> Result<RunProgress> {
    let conn = update.conn();
    let p = req.provider;
    let defs = resolve_metrics(p, &req.metrics)?;
    let ids = metric_ids(&defs);
    let fields = defs.len() as u64;
    let size = batch_size(p);
    let unit = unit_name(p);
    let params_json = serde_json::to_value(req).expect("serializes");
    let (run_id, mut counts, cap) = match resume {
        Some(id) => {
            let r =
                store::run(conn, id)?.ok_or_else(|| Error::Invalid(format!("no run {id} in this crawl")))?;
            let cap: Option<i64> = conn.query_row(
                "SELECT usage_cap FROM integration_runs WHERE id = ?1",
                [id],
                |r| r.get(0),
            )?;
            store::set_state(conn, id, RunState::Running, None, None)?;
            (
                id,
                Counts {
                    requested: r.requested,
                    done: r.done,
                    failed: r.failed,
                    matched: None,
                    unmatched: None,
                    usage: Some(r.usage.unwrap_or(0)),
                },
                cap.map_or(u64::MAX, |c| c.max(0) as u64),
            )
        }
        None => {
            let targets = target_urls(conn, req.url_set)?;
            let est = estimate_for(ctx, label, req, &defs, &targets);
            let cap = est.cap;
            conn.execute_batch("BEGIN IMMEDIATE")?;
            let created = (|| -> Result<i64> {
                conn.execute("DELETE FROM link_metrics WHERE provider = ?1", [p.id()])?;
                conn.execute("DELETE FROM integration_status WHERE provider = ?1", [p.id()])?;
                conn.execute(
                    "DELETE FROM integration_queue WHERE run_id IN
                     (SELECT id FROM integration_runs WHERE provider = ?1)",
                    [p.id()],
                )?;
                let id = store::create_run(conn, p, Some(label), None, &params_json)?;
                conn.execute(
                    "UPDATE integration_runs SET usage_cap = ?2, requested = ?3, usage = 0 WHERE id = ?1",
                    params![id, i64::try_from(cap).unwrap_or(i64::MAX), targets.len() as i64],
                )?;
                let mut ins = conn.prepare_cached(
                    "INSERT INTO integration_queue (run_id, url_id, pos, state) VALUES (?1, ?2, ?3, 'pending')",
                )?;
                for (pos, (uid, _)) in targets.iter().enumerate() {
                    ins.execute(params![id, uid, pos as i64])?;
                }
                Ok(id)
            })();
            let id = match created {
                Ok(id) => {
                    conn.execute_batch("COMMIT")?;
                    id
                }
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    return Err(e);
                }
            };
            (
                id,
                Counts {
                    requested: targets.len() as i64,
                    usage: Some(0),
                    ..Counts::default()
                },
                cap,
            )
        }
    };
    let explicit_cap = req.cap.is_some();
    report(conn, run_id, on_progress)?;
    // Units per page seen in the last full paid batch; until then the
    // estimate's own formula prices a batch.
    let mut rate: Option<f64> = None;
    let mut paid_batches = 0u32;
    loop {
        if control.cancelled() {
            store::save_progress(conn, run_id, &counts, None)?;
            store::set_state(conn, run_id, RunState::Cancelled, None, None)?;
            return report(conn, run_id, on_progress);
        }
        if control.paused() {
            store::save_progress(conn, run_id, &counts, None)?;
            store::set_state(conn, run_id, RunState::PausedUser, None, None)?;
            return report(conn, run_id, on_progress);
        }
        // Until the provider has shown what a page costs, a small first
        // batch keeps a surprise within the cap.
        let take = if paid_batches == 0 {
            size.min(PROBE_PAGES)
        } else {
            size
        };
        let batch = next_batch(conn, run_id, take)?;
        if batch.is_empty() {
            break;
        }
        let batch_len = batch.len() as u64;
        let mut answered: Vec<(i64, Map<String, Value>, i64)> = Vec::new();
        let mut miss: Vec<(i64, String)> = Vec::new();
        for (uid, url) in batch {
            let hit = ctx
                .cached(&cache_key(p, label, &url, &ids))
                .as_ref()
                .and_then(cached_answer);
            match hit {
                Some((m, t)) => answered.push((uid, m, t)),
                None => miss.push((uid, url)),
            }
        }
        let mut spent = 0u64;
        let mut stop: Option<Stop> = None;
        let used = counts.usage.unwrap_or(0).max(0) as u64;
        // Never ask for more than the cap leaves: a batch that would pass it
        // is cut down to the pages that still fit.
        let wanted = miss.len();
        let mut fit = wanted;
        while fit > 0 && used + price(p, rate, fields, fit) > cap {
            fit -= 1;
        }
        miss.truncate(fit);
        if fit < wanted {
            stop = Some(Stop::Cap);
        }
        if miss.is_empty() && answered.is_empty() {
            let msg = stop_message(&Stop::Cap, cap, unit, &counts);
            return finish_early(conn, run_id, &counts, &msg, on_progress);
        }
        if !miss.is_empty() {
            let projected = price(p, rate, fields, miss.len());
            let urls: Vec<&str> = miss.iter().map(|(_, u)| u.as_str()).collect();
            ctx.bucket.acquire().await;
            let (r, _) = with_retry(|| fetch_batch(ctx, p, secret, apis, &urls, &defs)).await;
            match r {
                Ok(out) => {
                    let now = Timestamp::now().as_second();
                    spent = out.units.unwrap_or(projected);
                    paid_batches += 1;
                    let floored = p == Provider::Ahrefs
                        && miss.len() as u64 * fields <= ahrefs::MIN_REQUEST_UNITS
                        && spent <= ahrefs::MIN_REQUEST_UNITS;
                    if !floored && (rate.is_none() || miss.len() >= size) {
                        rate = Some(spent as f64 / miss.len() as f64);
                    }
                    if paid_batches == 1 && !explicit_cap && stop.is_none() {
                        // Re-estimate from what the provider really charged
                        // (FR-029): every page still waiting, at this rate.
                        let waiting = pending(conn, run_id)?.saturating_sub(batch_len);
                        let per_page =
                            rate.unwrap_or_else(|| batch_cost(p, size as u64, fields) as f64 / size as f64);
                        let total = used + spent + (per_page * waiting as f64).ceil() as u64;
                        if total > cap {
                            stop = Some(Stop::Reestimate(total));
                        }
                    }
                    for ((uid, url), row) in miss.iter().zip(out.rows) {
                        let m = row.unwrap_or_default();
                        // Even "no data" is cached: asking again costs units.
                        ctx.remember(&cache_key(p, label, url, &ids), &json!({"m": m, "t": now}));
                        answered.push((*uid, m, now));
                    }
                }
                Err(e) => {
                    let (state, resume_at) = match &e {
                        HttpError::RateLimited {
                            quota: true,
                            retry_after,
                            ..
                        } => (
                            RunState::PausedQuota,
                            (*retry_after).map(|d| {
                                Timestamp::now() + jiff::SignedDuration::try_from(d).unwrap_or_default()
                            }),
                        ),
                        _ => (RunState::Failed, None),
                    };
                    store::save_progress(conn, run_id, &counts, None)?;
                    store::set_state(conn, run_id, state, Some(&e.to_string()), resume_at)?;
                    return report(conn, run_id, on_progress);
                }
            }
        }
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let written = write_batch(conn, run_id, p, &answered);
        if let Err(e) = written {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(e);
        }
        counts.done += answered.len() as i64;
        counts.usage = Some(counts.usage.unwrap_or(0) + i64::try_from(spent).unwrap_or(i64::MAX));
        store::save_progress(conn, run_id, &counts, None)?;
        conn.execute_batch("COMMIT")?;
        report(conn, run_id, on_progress)?;
        if let Some(why) = stop {
            let msg = stop_message(&why, cap, unit, &counts);
            return finish_early(conn, run_id, &counts, &msg, on_progress);
        }
    }
    conn.execute_batch("BEGIN IMMEDIATE")?;
    store::save_progress(conn, run_id, &counts, None)?;
    store::set_state(conn, run_id, RunState::Finished, None, None)?;
    conn.execute_batch("COMMIT")?;
    report(conn, run_id, on_progress)
}

fn write_batch(
    conn: &Connection,
    run_id: i64,
    p: Provider,
    answered: &[(i64, Map<String, Value>, i64)],
) -> Result<()> {
    let mut put = conn.prepare_cached(
        "INSERT OR REPLACE INTO link_metrics (url_id, provider, run_id, metrics, fetched_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    let mut none = conn.prepare_cached(
        "INSERT INTO integration_status (url_id, provider, run_id, reason)
         VALUES (?1, ?2, ?3, 'no_data')
         ON CONFLICT(url_id, provider) DO UPDATE SET run_id = excluded.run_id, reason = 'no_data'",
    )?;
    let mut done =
        conn.prepare_cached("UPDATE integration_queue SET state = 'done' WHERE run_id = ?1 AND url_id = ?2")?;
    for (uid, m, at) in answered {
        if m.is_empty() {
            none.execute(params![uid, p.id(), run_id])?;
        } else {
            put.execute(params![
                uid,
                p.id(),
                run_id,
                Value::Object(m.clone()).to_string(),
                at
            ])?;
        }
        done.execute(params![run_id, uid])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_follow_each_providers_own_pricing() {
        // Ahrefs: at least 50 units a request, otherwise one per page and metric.
        assert_eq!(estimate_units(Provider::Ahrefs, 10, 4), (1, 50));
        // A first batch of 25, two of 100 and one of 25.
        assert_eq!(estimate_units(Provider::Ahrefs, 250, 4), (4, 100 + 400 * 2 + 100));
        // Majestic units and Moz rows are one per page.
        assert_eq!(estimate_units(Provider::Majestic, 250, 4), (4, 250));
        assert_eq!(estimate_units(Provider::Moz, 120, 5), (3, 120));
        assert_eq!(estimate_units(Provider::Moz, 0, 5), (0, 0));
    }

    #[test]
    fn unknown_metrics_are_refused_with_the_choices() {
        let e = resolve_metrics(Provider::Moz, &["nope".into()])
            .unwrap_err()
            .to_string();
        assert!(e.contains("page_authority"), "{e}");
        assert_eq!(resolve_metrics(Provider::Ahrefs, &[]).unwrap().len(), 4);
        assert!(resolve_metrics(Provider::SearchConsole, &[]).is_err());
    }
}
