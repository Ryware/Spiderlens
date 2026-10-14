//! Search Console page data (spec 024 US1, plan D10).
//!
//! Pages come from `searchAnalytics.query` grouped by page (Google groups
//! by the canonical URL it chose), 25,000 rows per request, paged with
//! `startRow` until a short page. Each page of rows is matched and written in
//! one transaction together with the run's cursor, so a resumed run starts
//! exactly after the last page written.

use jiff::Timestamp;
use jiff::civil::Date;
use reqwest::Method;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use spiderlens_engine::crawl::Update;
use ts_rs::TS;

use crate::http::HttpError;
use crate::limits::{GOOGLE_TZ, next_reset, with_retry};
use crate::matching::Matcher;
use crate::oauth::Session;
use crate::run::{Control, Ctx};
use crate::store::{self, Counts, RunProgress, RunState};
use crate::{Error, Provider, Result, cache};

pub const ROW_LIMIT: usize = 25_000;

#[derive(Debug, Clone)]
pub struct Api {
    pub base: String,
}

impl Default for Api {
    fn default() -> Self {
        Self {
            base: "https://www.googleapis.com/webmasters/v3".into(),
        }
    }
}

/// A property the account can read, as the property picker shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Property {
    /// `sc-domain:example.com` or `https://example.com/`.
    pub id: String,
    pub display: String,
    pub kind: PropertyKind,
    /// Covers the crawled site; the best one is listed first.
    pub suggested: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PropertyKind {
    Domain,
    UrlPrefix,
}

/// What a page-data run asks for (FR-020).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PageRequest {
    pub property: String,
    /// Inclusive, `YYYY-MM-DD`.
    pub start_date: String,
    pub end_date: String,
    /// `web` by default; also image, video, news, discover, googleNews.
    pub search_type: String,
    /// Include fresh, still-changing data (labelled as such).
    pub fresh: bool,
}

/// The default range: the last 28 days of final data. Final data trails by
/// about three days.
pub fn default_range(today: Date) -> (Date, Date) {
    let end = today.checked_sub(jiff::Span::new().days(3)).unwrap_or(today);
    let start = end.checked_sub(jiff::Span::new().days(27)).unwrap_or(end);
    (start, end)
}

impl PageRequest {
    pub fn last_28_days(property: impl Into<String>) -> Self {
        let today = Timestamp::now()
            .in_tz(GOOGLE_TZ)
            .map(|z| z.date())
            .unwrap_or_else(|_| Timestamp::now().to_zoned(jiff::tz::TimeZone::UTC).date());
        let (s, e) = default_range(today);
        Self {
            property: property.into(),
            start_date: s.to_string(),
            end_date: e.to_string(),
            search_type: "web".into(),
            fresh: false,
        }
    }
}

/// Calls the API with the session's token: refreshes once on 401 and
/// applies spec 004's retry rules to transient failures.
async fn call(
    ctx: &Ctx,
    session: &Session,
    method: Method,
    url: &str,
    body: Option<&Value>,
) -> (std::result::Result<Value, HttpError>, u32) {
    let mut refreshed = false;
    loop {
        ctx.bucket.acquire().await;
        let (r, attempts) = with_retry(|| async {
            let token = session.bearer().await?;
            let mut req = ctx.http.request(method.clone(), url).bearer_auth(token);
            if let Some(b) = body {
                req = req
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .body(b.to_string());
            }
            ctx.http.json("Search Console", req).await
        })
        .await;
        match r {
            Err(HttpError::Unauthorized { .. }) if !refreshed => {
                session.invalidate();
                refreshed = true;
            }
            other => return (other, attempts),
        }
    }
}

/// The properties the account can read, best match for `crawl_host` first.
pub async fn properties(
    ctx: &Ctx,
    session: &Session,
    api: &Api,
    crawl_host: Option<&str>,
) -> Result<Vec<Property>> {
    let (r, _) = call(ctx, session, Method::GET, &format!("{}/sites", api.base), None).await;
    let v = r?;
    let mut list: Vec<Property> = v["siteEntry"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["permissionLevel"].as_str() != Some("siteUnverifiedUser"))
        .filter_map(|e| e["siteUrl"].as_str())
        .map(|id| {
            let (kind, display) = match id.strip_prefix("sc-domain:") {
                Some(d) => (PropertyKind::Domain, format!("{d} (domain)")),
                None => (PropertyKind::UrlPrefix, id.to_string()),
            };
            Property {
                id: id.to_string(),
                display,
                kind,
                suggested: false,
            }
        })
        .collect();
    if let Some(host) = crawl_host
        && let Some(best) = suggest(&list, host)
    {
        for p in &mut list {
            p.suggested = p.id == best;
        }
    }
    list.sort_by(|a, b| b.suggested.cmp(&a.suggested).then(a.display.cmp(&b.display)));
    Ok(list)
}

/// The property that covers `start_url`: a domain property for its host or
/// a parent domain, else the longest URL-prefix property it starts with.
pub fn suggest(props: &[Property], start_url: &str) -> Option<String> {
    let url = url::Url::parse(start_url)
        .ok()
        .or_else(|| url::Url::parse(&format!("https://{start_url}/")).ok())?;
    let host = url.host_str()?.to_ascii_lowercase();
    let domain = props
        .iter()
        .filter(|p| p.kind == PropertyKind::Domain)
        .filter_map(|p| {
            p.id.strip_prefix("sc-domain:")
                .map(|d| (p, d.to_ascii_lowercase()))
        })
        .filter(|(_, d)| host == *d || host.ends_with(&format!(".{d}")))
        .max_by_key(|(_, d)| d.len());
    if let Some((p, _)) = domain {
        return Some(p.id.clone());
    }
    let full = url.as_str().to_ascii_lowercase();
    props
        .iter()
        .filter(|p| p.kind == PropertyKind::UrlPrefix && full.starts_with(&p.id.to_ascii_lowercase()))
        .max_by_key(|p| p.id.len())
        .map(|p| p.id.clone())
}

fn query_url(api: &Api, property: &str) -> String {
    let encoded: String = url::form_urlencoded::byte_serialize(property.as_bytes()).collect();
    format!("{}/sites/{encoded}/searchAnalytics/query", api.base)
}

/// Starts a page-data run, or resumes `resume` from its cursor (FR-005).
#[allow(clippy::too_many_arguments)]
pub async fn run_pages(
    ctx: &Ctx,
    session: &Session,
    api: &Api,
    update: &Update,
    account_label: &str,
    req: &PageRequest,
    resume: Option<i64>,
    control: &Control,
    on_progress: &mut dyn FnMut(&RunProgress),
) -> Result<RunProgress> {
    let conn = update.conn();
    let params_json = serde_json::to_value(req).expect("serializes");
    let (run_id, mut start_row, mut counts) = match resume {
        Some(id) => {
            let p =
                store::run(conn, id)?.ok_or_else(|| Error::Invalid(format!("no run {id} in this crawl")))?;
            let cur = store::cursor(conn, id)?
                .and_then(|c| c.parse::<usize>().ok())
                .unwrap_or(0);
            store::set_state(conn, id, RunState::Running, None, None)?;
            (
                id,
                cur,
                Counts {
                    requested: p.requested,
                    done: p.done,
                    failed: p.failed,
                    matched: p.matched,
                    unmatched: p.unmatched,
                    usage: p.usage,
                },
            )
        }
        None => {
            conn.execute_batch("BEGIN IMMEDIATE")?;
            conn.execute("DELETE FROM gsc_pages", [])?;
            conn.execute("DELETE FROM gsc_queries", [])?;
            conn.execute(
                "DELETE FROM orphan_candidates WHERE source = 'search_console'",
                [],
            )?;
            conn.execute(
                "DELETE FROM integration_status WHERE provider = 'search_console'",
                [],
            )?;
            let id = store::create_run(
                conn,
                Provider::SearchConsole,
                Some(account_label),
                Some(&req.property),
                &params_json,
            )?;
            conn.execute_batch("COMMIT")?;
            (
                id,
                0,
                Counts {
                    matched: Some(0),
                    unmatched: Some(0),
                    usage: Some(0),
                    ..Counts::default()
                },
            )
        }
    };
    let report = |on_progress: &mut dyn FnMut(&RunProgress)| -> Result<RunProgress> {
        let p = store::run(conn, run_id)?.expect("run exists");
        on_progress(&p);
        Ok(p)
    };
    report(on_progress)?;
    let matcher = Matcher::build(conn)?;
    let url = query_url(api, &req.property);
    loop {
        if control.cancelled() {
            store::set_state(conn, run_id, RunState::Cancelled, None, None)?;
            return report(on_progress);
        }
        if control.paused() {
            store::set_state(conn, run_id, RunState::PausedUser, None, None)?;
            return report(on_progress);
        }
        let body = json!({
            "startDate": req.start_date,
            "endDate": req.end_date,
            "dimensions": ["page"],
            "type": req.search_type,
            "aggregationType": "byPage",
            "dataState": if req.fresh { "all" } else { "final" },
            "rowLimit": ROW_LIMIT,
            "startRow": start_row,
        });
        let key = cache::key(&["search_console", account_label, &url, &body.to_string()]);
        let page = match ctx.cached(&key) {
            Some(v) => v,
            None => {
                let (r, _) = call(ctx, session, Method::POST, &url, Some(&body)).await;
                match r {
                    Ok(v) => {
                        counts.usage = Some(counts.usage.unwrap_or(0) + 1);
                        ctx.remember(&key, &v);
                        v
                    }
                    Err(e) => {
                        let (state, resume_at) = match &e {
                            HttpError::RateLimited {
                                quota: true,
                                retry_after,
                                ..
                            } => (
                                RunState::PausedQuota,
                                Some(match retry_after {
                                    Some(d) => {
                                        Timestamp::now()
                                            + jiff::SignedDuration::try_from(*d).unwrap_or_default()
                                    }
                                    None => next_reset(Timestamp::now(), GOOGLE_TZ),
                                }),
                            ),
                            _ => (RunState::Failed, None),
                        };
                        store::save_progress(conn, run_id, &counts, None)?;
                        store::set_state(conn, run_id, state, Some(&e.to_string()), resume_at)?;
                        return report(on_progress);
                    }
                }
            }
        };
        let rows = page["rows"].as_array().cloned().unwrap_or_default();
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let written = write_rows(conn, &matcher, run_id, &rows, &mut counts);
        if let Err(e) = written {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(e);
        }
        start_row += rows.len();
        counts.requested = start_row as i64;
        counts.done = start_row as i64;
        store::save_progress(conn, run_id, &counts, Some(&start_row.to_string()))?;
        conn.execute_batch("COMMIT")?;
        report(on_progress)?;
        if rows.len() < ROW_LIMIT {
            break;
        }
    }
    conn.execute_batch("BEGIN IMMEDIATE")?;
    // Crawled pages Google has no rows for: zero, and marked "no data" so
    // they read differently from "not fetched" (US1.3).
    conn.execute(
        "INSERT INTO integration_status (url_id, provider, run_id, reason)
         SELECT p.url_id, 'search_console', ?1, 'no_data' FROM pages p JOIN urls u ON u.id = p.url_id
         WHERE u.internal = 1 AND NOT EXISTS (SELECT 1 FROM gsc_pages g WHERE g.url_id = p.url_id)
         ON CONFLICT(url_id, provider) DO UPDATE SET run_id = excluded.run_id, reason = 'no_data'",
        [run_id],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO gsc_pages (url_id, run_id, clicks, impressions, ctr, position)
         SELECT p.url_id, ?1, 0, 0, 0, 0 FROM pages p JOIN urls u ON u.id = p.url_id WHERE u.internal = 1",
        [run_id],
    )?;
    store::set_state(conn, run_id, RunState::Finished, None, None)?;
    conn.execute_batch("COMMIT")?;
    report(on_progress)
}

fn write_rows(
    conn: &Connection,
    matcher: &Matcher<'_>,
    run_id: i64,
    rows: &[Value],
    counts: &mut Counts,
) -> Result<()> {
    let mut page_stmt = conn.prepare_cached(
        "INSERT INTO gsc_pages (url_id, run_id, clicks, impressions, ctr, position)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(url_id) DO UPDATE SET
           position = CASE WHEN gsc_pages.impressions + excluded.impressions > 0
             THEN (gsc_pages.position * gsc_pages.impressions + excluded.position * excluded.impressions)
                  / (gsc_pages.impressions + excluded.impressions)
             ELSE gsc_pages.position END,
           clicks = gsc_pages.clicks + excluded.clicks,
           impressions = gsc_pages.impressions + excluded.impressions,
           ctr = CASE WHEN gsc_pages.impressions + excluded.impressions > 0
             THEN CAST(gsc_pages.clicks + excluded.clicks AS REAL) / (gsc_pages.impressions + excluded.impressions)
             ELSE 0 END",
    )?;
    let mut orphan_stmt = conn.prepare_cached(
        "INSERT INTO orphan_candidates (url, source, run_id, clicks, impressions)
         VALUES (?1, 'search_console', ?2, ?3, ?4)
         ON CONFLICT(url, source) DO UPDATE SET clicks = clicks + excluded.clicks,
           impressions = impressions + excluded.impressions",
    )?;
    for row in rows {
        let Some(page) = row["keys"].get(0).and_then(Value::as_str) else {
            continue;
        };
        let clicks = row["clicks"].as_f64().unwrap_or(0.0).round() as i64;
        let impressions = row["impressions"].as_f64().unwrap_or(0.0).round() as i64;
        let ctr = row["ctr"].as_f64().unwrap_or(0.0);
        let position = row["position"].as_f64().unwrap_or(0.0);
        match matcher.lookup(page)? {
            Some(id) => {
                page_stmt.execute(params![id, run_id, clicks, impressions, ctr, position])?;
                counts.matched = Some(counts.matched.unwrap_or(0) + 1);
            }
            None => {
                orphan_stmt.execute(params![page, run_id, clicks, impressions])?;
                counts.unmatched = Some(counts.unmatched.unwrap_or(0) + 1);
            }
        }
    }
    Ok(())
}

/// One query row for the inspector (US1.6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct QueryRow {
    pub query: String,
    #[ts(type = "number")]
    pub clicks: i64,
    #[ts(type = "number")]
    pub impressions: i64,
    pub ctr: f64,
    pub position: f64,
}

/// The page's top queries for the latest finished page-data run's range,
/// fetched on demand, cached and stored (US1.6).
pub async fn top_queries(
    ctx: &Ctx,
    session: &Session,
    api: &Api,
    conn: &Connection,
    account_label: &str,
    url_id: i64,
) -> Result<Vec<QueryRow>> {
    let run = store::runs(conn)?
        .into_iter()
        .find(|r| r.provider == Provider::SearchConsole && r.state == RunState::Finished)
        .ok_or_else(|| Error::Invalid("fetch Search Console data for this crawl first".into()))?;
    let req: PageRequest =
        serde_json::from_value(run.params.clone()).map_err(|e| Error::Invalid(e.to_string()))?;
    let page_url: String = conn.query_row("SELECT url FROM urls WHERE id = ?1", [url_id], |r| r.get(0))?;
    let body = json!({
        "startDate": req.start_date,
        "endDate": req.end_date,
        "dimensions": ["query"],
        "type": req.search_type,
        "dataState": if req.fresh { "all" } else { "final" },
        "dimensionFilterGroups": [{"filters": [{"dimension": "page", "operator": "equals", "expression": page_url}]}],
        "rowLimit": 100,
    });
    let url = query_url(api, &req.property);
    let key = cache::key(&["search_console", account_label, &url, &body.to_string()]);
    let v = match ctx.cached(&key) {
        Some(v) => v,
        None => {
            let (r, _) = call(ctx, session, Method::POST, &url, Some(&body)).await;
            let v = r?;
            ctx.remember(&key, &v);
            v
        }
    };
    let rows: Vec<QueryRow> = v["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            Some(QueryRow {
                query: r["keys"].get(0)?.as_str()?.to_string(),
                clicks: r["clicks"].as_f64().unwrap_or(0.0).round() as i64,
                impressions: r["impressions"].as_f64().unwrap_or(0.0).round() as i64,
                ctr: r["ctr"].as_f64().unwrap_or(0.0),
                position: r["position"].as_f64().unwrap_or(0.0),
            })
        })
        .collect();
    conn.execute_batch("BEGIN IMMEDIATE")?;
    conn.execute("DELETE FROM gsc_queries WHERE url_id = ?1", [url_id])?;
    for q in &rows {
        conn.execute(
            "INSERT OR REPLACE INTO gsc_queries (url_id, run_id, query, clicks, impressions, ctr, position)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                url_id,
                run.id,
                q.query,
                q.clicks,
                q.impressions,
                q.ctr,
                q.position
            ],
        )?;
    }
    conn.execute_batch("COMMIT")?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(id: &str) -> Property {
        Property {
            id: id.into(),
            display: id.into(),
            kind: if id.starts_with("sc-domain:") {
                PropertyKind::Domain
            } else {
                PropertyKind::UrlPrefix
            },
            suggested: false,
        }
    }

    #[test]
    fn suggests_domain_property_then_longest_prefix() {
        let props = [
            p("https://www.a.test/"),
            p("https://www.a.test/blog/"),
            p("sc-domain:a.test"),
            p("https://b.test/"),
        ];
        assert_eq!(
            suggest(&props, "https://www.a.test/blog/x").as_deref(),
            Some("sc-domain:a.test")
        );
        let props = [p("https://www.a.test/"), p("https://www.a.test/blog/")];
        assert_eq!(
            suggest(&props, "https://www.a.test/blog/x").as_deref(),
            Some("https://www.a.test/blog/")
        );
        assert_eq!(suggest(&props, "https://c.test/").as_deref(), None);
        // A domain property for a different registrable domain never matches.
        assert_eq!(
            suggest(&[p("sc-domain:test")], "https://nota.test/").as_deref(),
            Some("sc-domain:test")
        );
        assert_eq!(
            suggest(&[p("sc-domain:b.test")], "https://ab.test/").as_deref(),
            None
        );
    }

    #[test]
    fn default_range_is_28_days_ending_three_days_ago() {
        let (s, e) = default_range("2026-10-14".parse().unwrap());
        assert_eq!(e.to_string(), "2026-10-11");
        assert_eq!(s.to_string(), "2026-09-14");
    }
}
