//! Google Analytics 4 sessions per landing page (spec 024 US4, plan D10).
//!
//! The Admin API lists the account's properties and web data streams so the
//! user can pick one (pre-selected when a stream's domain is the crawled
//! host). The Data API's `runReport` returns sessions, engaged sessions,
//! engagement rate, key events and revenue per host and landing page,
//! paged with `offset`. Each page of rows is matched to crawled URLs and
//! written in one transaction together with the run's cursor. Landing pages
//! that differ only by query string are combined by default (the
//! `landingPage` dimension); `keep_query` reports them apart
//! (`landingPagePlusQueryString`). The `propertyQuota` of every answer says
//! how many tokens are left; a run stops before the allowance runs out and
//! continues once it is back (FR-024).

use jiff::civil::Date;
use jiff::{SignedDuration, Timestamp};
use reqwest::Method;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use spiderlens_engine::crawl::Update;
use ts_rs::TS;

use crate::gcall::{call, in_tx};
use crate::http::HttpError;
use crate::limits::{GOOGLE_TZ, next_reset};
use crate::matching::Matcher;
use crate::oauth::Session;
use crate::run::{Control, Ctx};
use crate::store::{self, Counts, RunProgress, RunState};
use crate::{Error, Provider, Result, cache};

/// Rows asked per `runReport` request (the API allows up to 250,000).
pub const ROW_LIMIT: u32 = 100_000;

const NAME: &str = "Google Analytics";

#[derive(Debug, Clone)]
pub struct Ga4Api {
    pub admin_base: String,
    pub data_base: String,
}

impl Default for Ga4Api {
    fn default() -> Self {
        Self {
            admin_base: "https://analyticsadmin.googleapis.com/v1beta".into(),
            data_base: "https://analyticsdata.googleapis.com/v1beta".into(),
        }
    }
}

/// A web data stream the account can read, as the picker shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Ga4Stream {
    /// `properties/123456`.
    pub property: String,
    pub property_name: String,
    /// The stream's id, the last part of its resource name.
    pub stream: String,
    pub stream_name: String,
    /// The site address the stream was set up for.
    pub default_uri: String,
    pub display: String,
    /// The stream's domain is the crawled host; the best one is listed first.
    pub suggested: bool,
}

/// What a landing-page run asks for (FR-024).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Ga4Request {
    /// `properties/123456`.
    pub property: String,
    /// The web data stream's id.
    pub stream: String,
    /// Inclusive, `YYYY-MM-DD`.
    pub start_date: String,
    pub end_date: String,
    /// Report landing pages that differ by query string apart (default:
    /// combined).
    #[serde(default)]
    pub keep_query: bool,
    /// Skip cached answers.
    #[serde(default)]
    pub fresh: bool,
    /// Rows per request; the API maximum when none.
    #[serde(default)]
    pub page_size: Option<u32>,
}

/// The default range: the last 28 days up to yesterday.
pub fn default_range(today: Date) -> (Date, Date) {
    let end = today.checked_sub(jiff::Span::new().days(1)).unwrap_or(today);
    let start = end.checked_sub(jiff::Span::new().days(27)).unwrap_or(end);
    (start, end)
}

impl Ga4Request {
    /// `property` may be given as `properties/123` or just `123`.
    pub fn last_28_days(property: impl Into<String>, stream: impl Into<String>) -> Self {
        let property = property.into();
        let property = if property.starts_with("properties/") {
            property
        } else {
            format!("properties/{property}")
        };
        let today = Timestamp::now().to_zoned(jiff::tz::TimeZone::UTC).date();
        let (s, e) = default_range(today);
        Self {
            property,
            stream: stream.into(),
            start_date: s.to_string(),
            end_date: e.to_string(),
            keep_query: false,
            fresh: false,
            page_size: None,
        }
    }
}

/// The host of a site address, without `www.`, for comparing streams with
/// the crawled site.
fn host_of(uri: &str) -> Option<String> {
    let url = url::Url::parse(uri)
        .ok()
        .filter(|u| u.host_str().is_some())
        .or_else(|| url::Url::parse(&format!("https://{uri}")).ok())?;
    Some(url.host_str()?.trim_start_matches("www.").to_ascii_lowercase())
}

/// Marks the stream whose domain is the crawled host, when exactly one is.
pub fn suggest(streams: &mut [Ga4Stream], start_url: &str) {
    let Some(wanted) = host_of(start_url) else {
        return;
    };
    let hits: Vec<usize> = streams
        .iter()
        .enumerate()
        .filter(|(_, s)| host_of(&s.default_uri).as_deref() == Some(wanted.as_str()))
        .map(|(i, _)| i)
        .collect();
    if let [one] = hits.as_slice() {
        streams[*one].suggested = true;
    }
}

fn paged_url(base: &str, token: Option<&str>) -> String {
    let mut url = format!("{base}?pageSize=200");
    if let Some(t) = token {
        url.push_str("&pageToken=");
        url.extend(url::form_urlencoded::byte_serialize(t.as_bytes()));
    }
    url
}

/// The web data streams of every property the account can read, the one
/// for `crawl_start_url`'s host first.
pub async fn streams(
    ctx: &Ctx,
    session: &Session,
    api: &Ga4Api,
    crawl_start_url: Option<&str>,
) -> Result<Vec<Ga4Stream>> {
    let mut properties: Vec<(String, String)> = Vec::new();
    let mut token: Option<String> = None;
    loop {
        let url = paged_url(&format!("{}/accountSummaries", api.admin_base), token.as_deref());
        let (r, _) = call(ctx, session, NAME, Method::GET, &url, None).await;
        let v = r?;
        for account in v["accountSummaries"].as_array().into_iter().flatten() {
            for p in account["propertySummaries"].as_array().into_iter().flatten() {
                if let Some(id) = p["property"].as_str() {
                    let name = p["displayName"].as_str().unwrap_or(id);
                    properties.push((id.to_string(), name.to_string()));
                }
            }
        }
        token = v["nextPageToken"]
            .as_str()
            .filter(|t| !t.is_empty())
            .map(str::to_string);
        if token.is_none() {
            break;
        }
    }
    let mut out = Vec::new();
    for (property, property_name) in properties {
        let url = paged_url(&format!("{}/{property}/dataStreams", api.admin_base), None);
        let (r, _) = call(ctx, session, NAME, Method::GET, &url, None).await;
        let v = match r {
            Ok(v) => v,
            // A property the account can list but not read streams of.
            Err(HttpError::Forbidden { .. } | HttpError::NotFound { .. }) => continue,
            Err(e) => return Err(e.into()),
        };
        for s in v["dataStreams"].as_array().into_iter().flatten() {
            if s["type"].as_str() != Some("WEB_DATA_STREAM") {
                continue;
            }
            let Some(name) = s["name"].as_str() else {
                continue;
            };
            let stream = name.rsplit('/').next().unwrap_or(name).to_string();
            let stream_name = s["displayName"].as_str().unwrap_or(&stream).to_string();
            let default_uri = s
                .pointer("/webStreamData/defaultUri")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            out.push(Ga4Stream {
                display: format!("{property_name}: {stream_name} ({default_uri})"),
                property: property.clone(),
                property_name: property_name.clone(),
                stream,
                stream_name,
                default_uri,
                suggested: false,
            });
        }
    }
    if let Some(start) = crawl_start_url {
        suggest(&mut out, start);
    }
    out.sort_by(|a, b| {
        b.suggested
            .cmp(&a.suggested)
            .then(a.property_name.cmp(&b.property_name))
            .then(a.stream_name.cmp(&b.stream_name))
    });
    Ok(out)
}

fn report_url(api: &Ga4Api, property: &str) -> String {
    format!("{}/{property}:runReport", api.data_base)
}

/// One landing page row.
#[derive(Debug, Clone, PartialEq)]
pub struct LandingRow {
    pub url: String,
    pub sessions: i64,
    pub engaged_sessions: i64,
    pub engagement_rate: f64,
    pub key_events: f64,
    pub revenue: f64,
}

fn metric(row: &Value, i: usize) -> f64 {
    row["metricValues"]
        .get(i)
        .and_then(|m| m["value"].as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// Reads a report row: host and landing page, then the five metrics.
/// Rows without a real page (`(not set)`) have no address.
pub fn parse_row(row: &Value) -> Option<LandingRow> {
    let dim = |i: usize| row["dimensionValues"].get(i).and_then(|d| d["value"].as_str());
    let (host, path) = (dim(0)?, dim(1)?);
    if host.is_empty() || host.starts_with('(') || !path.starts_with('/') {
        return None;
    }
    Some(LandingRow {
        url: format!("https://{host}{path}"),
        sessions: metric(row, 0).round() as i64,
        engaged_sessions: metric(row, 1).round() as i64,
        engagement_rate: metric(row, 2),
        key_events: metric(row, 3),
        revenue: metric(row, 4),
    })
}

/// When the next request can be afforded, from a `propertyQuota` block: the
/// request just made cost `consumed` tokens, so another needs that many
/// left. Daily wins over hourly.
pub fn quota_wait(quota: &Value, now: Timestamp) -> Option<(Timestamp, String)> {
    let short = |name: &str| -> Option<(i64, i64)> {
        let q = quota.get(name)?;
        Some((q["remaining"].as_i64()?, q["consumed"].as_i64().unwrap_or(0)))
    };
    if let Some((remaining, cost)) = short("tokensPerDay")
        && remaining < cost.max(1)
    {
        let at = next_reset(now, GOOGLE_TZ);
        return Some((
            at,
            format!("Analytics' daily token allowance for this property is used up ({remaining} left)"),
        ));
    }
    if let Some((remaining, cost)) = short("tokensPerHour")
        && remaining < cost.max(1)
    {
        return Some((
            now + SignedDuration::from_hours(1),
            format!("Analytics' hourly token allowance for this property is used up ({remaining} left)"),
        ));
    }
    None
}

/// Starts a landing-page run, or resumes `resume` from its offset.
#[allow(clippy::too_many_arguments)]
pub async fn run_pages(
    ctx: &Ctx,
    session: &Session,
    api: &Ga4Api,
    update: &Update,
    account_label: &str,
    req: &Ga4Request,
    resume: Option<i64>,
    control: &Control,
    on_progress: &mut dyn FnMut(&RunProgress),
) -> Result<RunProgress> {
    let conn = update.conn();
    let params_json = serde_json::to_value(req).expect("serializes");
    let (run_id, mut offset, mut counts) = match resume {
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
        None => in_tx(conn, || {
            conn.execute("DELETE FROM ga4_pages", [])?;
            conn.execute("DELETE FROM orphan_candidates WHERE source = 'analytics'", [])?;
            conn.execute("DELETE FROM integration_status WHERE provider = 'analytics'", [])?;
            let id = store::create_run(
                conn,
                Provider::Analytics,
                Some(account_label),
                Some(&req.property),
                &params_json,
            )?;
            Ok((
                id,
                0,
                Counts {
                    matched: Some(0),
                    unmatched: Some(0),
                    usage: Some(0),
                    ..Counts::default()
                },
            ))
        })?,
    };
    let report = |on_progress: &mut dyn FnMut(&RunProgress)| -> Result<RunProgress> {
        let p = store::run(conn, run_id)?.expect("run exists");
        on_progress(&p);
        Ok(p)
    };
    report(on_progress)?;
    let matcher = Matcher::build(conn)?;
    let url = report_url(api, &req.property);
    let dimension = if req.keep_query {
        "landingPagePlusQueryString"
    } else {
        "landingPage"
    };
    let limit = req.page_size.unwrap_or(ROW_LIMIT).clamp(1, ROW_LIMIT) as usize;
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
            "dateRanges": [{"startDate": req.start_date, "endDate": req.end_date}],
            "dimensions": [{"name": "hostName"}, {"name": dimension}],
            "metrics": [
                {"name": "sessions"},
                {"name": "engagedSessions"},
                {"name": "engagementRate"},
                {"name": "keyEvents"},
                {"name": "totalRevenue"},
            ],
            "dimensionFilter": {"filter": {
                "fieldName": "streamId",
                "stringFilter": {"matchType": "EXACT", "value": req.stream},
            }},
            "orderBys": [
                {"metric": {"metricName": "sessions"}, "desc": true},
                {"dimension": {"dimensionName": "hostName"}},
                {"dimension": {"dimensionName": dimension}},
            ],
            "limit": limit,
            "offset": offset,
            "returnPropertyQuota": true,
        });
        let key = cache::key(&["analytics", account_label, &url, &body.to_string()]);
        let (page, from_cache) = match ctx.cached(&key) {
            Some(v) => (v, true),
            None => {
                let (r, _) = call(ctx, session, NAME, Method::POST, &url, Some(&body)).await;
                match r {
                    Ok(v) => {
                        ctx.remember(&key, &v);
                        (v, false)
                    }
                    Err(e) => {
                        let now = Timestamp::now();
                        let (state, resume_at) = match &e {
                            HttpError::RateLimited {
                                retry_after,
                                message,
                                ..
                            } => (
                                RunState::PausedQuota,
                                Some(match retry_after {
                                    Some(d) => now + SignedDuration::try_from(*d).unwrap_or_default(),
                                    None if message.to_ascii_lowercase().contains("day") => {
                                        next_reset(now, GOOGLE_TZ)
                                    }
                                    None => now + SignedDuration::from_hours(1),
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
        let total = page["rowCount"].as_i64();
        let consumed = page
            .pointer("/propertyQuota/tokensPerHour/consumed")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        in_tx(conn, || write_rows(conn, &matcher, run_id, &rows, &mut counts))?;
        offset += rows.len();
        counts.done = offset as i64;
        counts.requested = total.unwrap_or(counts.done).max(counts.done);
        if !from_cache {
            counts.usage = Some(counts.usage.unwrap_or(0) + consumed.max(1));
        }
        let counts_now = counts;
        in_tx(conn, || {
            store::save_progress(conn, run_id, &counts_now, Some(&offset.to_string()))
        })?;
        report(on_progress)?;
        let more = !rows.is_empty()
            && match total {
                Some(t) => (offset as i64) < t,
                None => rows.len() >= limit,
            };
        if !more {
            break;
        }
        if !from_cache
            && let Some((at, why)) = page
                .get("propertyQuota")
                .and_then(|q| quota_wait(q, Timestamp::now()))
        {
            let left = counts.requested - counts.done;
            let message = format!("{why}. {left} landing pages are left; the run continues after {at}.");
            store::set_state(conn, run_id, RunState::PausedQuota, Some(&message), Some(at))?;
            return report(on_progress);
        }
    }
    in_tx(conn, || {
        // Crawled pages Analytics has no row for: zero, and marked "no data"
        // so they read differently from "not fetched".
        conn.execute(
            "INSERT INTO integration_status (url_id, provider, run_id, reason)
             SELECT p.url_id, 'analytics', ?1, 'no_data' FROM pages p JOIN urls u ON u.id = p.url_id
             WHERE u.internal = 1 AND NOT EXISTS (SELECT 1 FROM ga4_pages g WHERE g.url_id = p.url_id)
             ON CONFLICT(url_id, provider) DO UPDATE SET run_id = excluded.run_id, reason = 'no_data'",
            [run_id],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO ga4_pages (url_id, run_id, sessions, engaged_sessions, engagement_rate, key_events, revenue)
             SELECT p.url_id, ?1, 0, 0, 0, 0, 0 FROM pages p JOIN urls u ON u.id = p.url_id WHERE u.internal = 1",
            [run_id],
        )?;
        store::set_state(conn, run_id, RunState::Finished, None, None)
    })?;
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
        "INSERT INTO ga4_pages (url_id, run_id, sessions, engaged_sessions, engagement_rate, key_events, revenue)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(url_id) DO UPDATE SET
           engagement_rate = CASE WHEN ga4_pages.sessions + excluded.sessions > 0
             THEN CAST(ga4_pages.engaged_sessions + excluded.engaged_sessions AS REAL)
                  / (ga4_pages.sessions + excluded.sessions)
             ELSE 0 END,
           sessions = ga4_pages.sessions + excluded.sessions,
           engaged_sessions = ga4_pages.engaged_sessions + excluded.engaged_sessions,
           key_events = COALESCE(ga4_pages.key_events, 0) + COALESCE(excluded.key_events, 0),
           revenue = COALESCE(ga4_pages.revenue, 0) + COALESCE(excluded.revenue, 0)",
    )?;
    let mut orphan_stmt = conn.prepare_cached(
        "INSERT INTO orphan_candidates (url, source, run_id, sessions)
         VALUES (?1, 'analytics', ?2, ?3)
         ON CONFLICT(url, source) DO UPDATE SET
           sessions = COALESCE(orphan_candidates.sessions, 0) + excluded.sessions",
    )?;
    for row in rows {
        let Some(r) = parse_row(row) else {
            continue;
        };
        match matcher.lookup(&r.url)? {
            Some(id) => {
                page_stmt.execute(params![
                    id,
                    run_id,
                    r.sessions,
                    r.engaged_sessions,
                    r.engagement_rate,
                    r.key_events,
                    r.revenue
                ])?;
                counts.matched = Some(counts.matched.unwrap_or(0) + 1);
            }
            None => {
                orphan_stmt.execute(params![r.url, run_id, r.sessions])?;
                counts.unmatched = Some(counts.unmatched.unwrap_or(0) + 1);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(property: &str, uri: &str) -> Ga4Stream {
        Ga4Stream {
            property: property.into(),
            property_name: property.into(),
            stream: "1".into(),
            stream_name: "web".into(),
            default_uri: uri.into(),
            display: String::new(),
            suggested: false,
        }
    }

    #[test]
    fn a_stream_is_suggested_only_when_exactly_one_matches_the_host() {
        let mut one = [
            stream("properties/1", "https://www.a.test"),
            stream("properties/2", "https://b.test"),
        ];
        suggest(&mut one, "https://a.test/blog/x");
        assert_eq!([one[0].suggested, one[1].suggested], [true, false]);
        let mut two = [stream("properties/1", "https://a.test"), stream("properties/2", "a.test")];
        suggest(&mut two, "https://a.test/");
        assert_eq!([two[0].suggested, two[1].suggested], [false, false]);
    }

    #[test]
    fn default_range_is_28_days_ending_yesterday() {
        let (s, e) = default_range("2026-10-14".parse().unwrap());
        assert_eq!(e.to_string(), "2026-10-13");
        assert_eq!(s.to_string(), "2026-09-16");
    }

    #[test]
    fn rows_become_full_addresses_and_skip_unset_pages() {
        let row = json!({
            "dimensionValues": [{"value": "a.test"}, {"value": "/blog/x?utm=1"}],
            "metricValues": [{"value": "12"}, {"value": "9"}, {"value": "0.75"}, {"value": "2"}, {"value": "30.5"}]
        });
        let r = parse_row(&row).unwrap();
        assert_eq!(r.url, "https://a.test/blog/x?utm=1");
        assert_eq!((r.sessions, r.engaged_sessions), (12, 9));
        assert_eq!((r.engagement_rate, r.key_events, r.revenue), (0.75, 2.0, 30.5));
        let unset = json!({"dimensionValues": [{"value": "a.test"}, {"value": "(not set)"}], "metricValues": []});
        assert!(parse_row(&unset).is_none());
    }

    #[test]
    fn quota_wait_stops_before_the_allowance_runs_out() {
        let now: Timestamp = "2026-10-14T10:00:00Z".parse().unwrap();
        let ok = json!({"tokensPerHour": {"consumed": 5, "remaining": 900}, "tokensPerDay": {"consumed": 5, "remaining": 9000}});
        assert!(quota_wait(&ok, now).is_none());
        let hourly = json!({"tokensPerHour": {"consumed": 5, "remaining": 3}, "tokensPerDay": {"consumed": 5, "remaining": 9000}});
        let (at, why) = quota_wait(&hourly, now).unwrap();
        assert_eq!(at.to_string(), "2026-10-14T11:00:00Z");
        assert!(why.contains("hourly"));
        let daily = json!({"tokensPerDay": {"consumed": 5, "remaining": 0}});
        let (at, why) = quota_wait(&daily, now).unwrap();
        assert_eq!(at.to_string(), "2026-10-15T07:00:00Z");
        assert!(why.contains("daily"));
    }
}
