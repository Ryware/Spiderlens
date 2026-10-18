//! Chrome UX Report field data (spec 024 US3, plan D10).
//!
//! `records:queryRecord` per page and form factor. A page with too little
//! real-user traffic answers 404; the origin's record is used then, fetched
//! once per origin and form factor, and stored with `level = origin` so it is
//! never shown as the page's own numbers. The API key travels in the
//! `x-goog-api-key` header, never in the address, so it cannot leak into an
//! error message.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use reqwest::Method;
use rusqlite::{Connection, params};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use spiderlens_engine::crawl::Update;
use ts_rs::TS;

use crate::http::HttpError;
use crate::limits::with_retry;
use crate::perf::{Allowance, Attempt, Driver, Item, PerfEstimate, UrlSet, begin, select_urls};
use crate::run::{Control, Ctx};
use crate::store::RunProgress;
use crate::{Error, Provider, Result, cache};

#[derive(Debug, Clone)]
pub struct Api {
    pub base: String,
}

impl Default for Api {
    fn default() -> Self {
        Self {
            base: "https://chromeuxreport.googleapis.com/v1".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum FormFactor {
    Phone,
    Desktop,
}

impl FormFactor {
    pub const BOTH: [FormFactor; 2] = [Self::Phone, Self::Desktop];

    pub fn id(self) -> &'static str {
        match self {
            Self::Phone => "phone",
            Self::Desktop => "desktop",
        }
    }

    fn api(self) -> &'static str {
        match self {
            Self::Phone => "PHONE",
            Self::Desktop => "DESKTOP",
        }
    }
}

/// What a field data run asks for (FR-025).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FieldRequest {
    pub devices: Vec<FormFactor>,
    pub urls: UrlSet,
    /// Keep only the first pages (shallowest first).
    pub max: Option<u32>,
    /// Ignore cached answers.
    #[serde(default)]
    pub fresh: bool,
}

impl Default for FieldRequest {
    fn default() -> Self {
        Self {
            devices: FormFactor::BOTH.to_vec(),
            urls: UrlSet::All,
            max: None,
            fresh: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    Lcp,
    Inp,
    Cls,
    Fcp,
    Ttfb,
}

/// The rating of a 75th percentile value by the Core Web Vitals thresholds
/// (web.dev): good up to the first limit, poor above the second.
pub fn rate(metric: Metric, value: f64) -> &'static str {
    let (good, poor) = match metric {
        Metric::Lcp => (2500.0, 4000.0),
        Metric::Inp => (200.0, 500.0),
        Metric::Cls => (0.1, 0.25),
        Metric::Fcp => (1800.0, 3000.0),
        Metric::Ttfb => (800.0, 1800.0),
    };
    if value <= good {
        "good"
    } else if value > poor {
        "poor"
    } else {
        "needs_improvement"
    }
}

/// The 75th percentiles of one record and the 28-day period they cover.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Record {
    pub lcp_ms: Option<i64>,
    pub inp_ms: Option<i64>,
    pub cls: Option<f64>,
    pub fcp_ms: Option<i64>,
    pub ttfb_ms: Option<i64>,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
}

fn p75(metric: &Value) -> Option<f64> {
    let v = metric.pointer("/percentiles/p75")?;
    // CLS arrives as a string, the others as numbers.
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

fn date(d: &Value) -> Option<String> {
    Some(format!(
        "{:04}-{:02}-{:02}",
        d["year"].as_i64()?,
        d["month"].as_i64()?,
        d["day"].as_i64()?
    ))
}

/// Reads a `queryRecord` answer; `None` when it holds no metric at all.
pub fn parse_record(v: &Value) -> Option<Record> {
    let metrics = v.pointer("/record/metrics")?;
    let get = |names: &[&str]| names.iter().find_map(|n| metrics.get(*n)).and_then(p75);
    let rec = Record {
        lcp_ms: get(&["largest_contentful_paint"]).map(|x| x.round() as i64),
        inp_ms: get(&["interaction_to_next_paint"]).map(|x| x.round() as i64),
        cls: get(&["cumulative_layout_shift"]),
        fcp_ms: get(&["first_contentful_paint"]).map(|x| x.round() as i64),
        ttfb_ms: get(&["experimental_time_to_first_byte", "time_to_first_byte"]).map(|x| x.round() as i64),
        period_start: v.pointer("/record/collectionPeriod/firstDate").and_then(date),
        period_end: v.pointer("/record/collectionPeriod/lastDate").and_then(date),
    };
    let any = rec.lcp_ms.is_some()
        || rec.inp_ms.is_some()
        || rec.cls.is_some()
        || rec.fcp_ms.is_some()
        || rec.ttfb_ms.is_some();
    any.then_some(rec)
}

/// What field data was found for a page on one device.
#[derive(Debug, Clone, PartialEq)]
pub enum Found {
    /// The page's own record.
    Url(Record),
    /// The origin's record, because the page has too little traffic.
    Origin(Record),
    None,
}

/// Looks one target up, from the cache or the API. A 404 is an answer, not
/// a failure: it is cached too, so the same day's second run asks nothing.
/// Returns the record (if any), the attempts, and the requests sent.
async fn lookup(
    ctx: &Ctx,
    key: &SecretString,
    api: &Api,
    label: &str,
    kind: &str,
    target: &str,
    device: FormFactor,
) -> (std::result::Result<Option<Value>, HttpError>, u32, u64) {
    let ck = cache::key(&["field_data", label, kind, target, device.id()]);
    if let Some(v) = ctx.cached(&ck) {
        let none = v.get("none").is_some();
        return (Ok((!none).then_some(v)), 0, 0);
    }
    let url = format!("{}/records:queryRecord", api.base);
    let mut body = serde_json::Map::new();
    body.insert(kind.to_string(), json!(target));
    body.insert("formFactor".to_string(), json!(device.api()));
    let body = Value::Object(body);
    let (r, attempts) = with_retry(|| async {
        ctx.bucket.acquire().await;
        let req = ctx
            .http
            .request(Method::POST, &url)
            .header("x-goog-api-key", key.expose_secret())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string());
        ctx.http.json("Chrome UX Report", req).await
    })
    .await;
    let sent = u64::from(attempts);
    match r {
        Ok(v) => {
            ctx.remember(&ck, &v);
            (Ok(Some(v)), attempts, sent)
        }
        Err(HttpError::NotFound { .. }) => {
            ctx.remember(&ck, &json!({ "none": true }));
            (Ok(None), attempts, sent)
        }
        Err(e) => (Err(e), attempts, sent),
    }
}

/// Origin answers already found in this run, so each origin and device is
/// asked once however many pages fall back to it. A failed lookup is not
/// remembered: the next page tries again.
type Slot = Rc<tokio::sync::Mutex<Option<Option<Value>>>>;

#[derive(Default)]
struct Origins(RefCell<HashMap<(String, FormFactor), Slot>>);

impl Origins {
    fn slot(&self, origin: &str, device: FormFactor) -> Slot {
        self.0
            .borrow_mut()
            .entry((origin.to_string(), device))
            .or_default()
            .clone()
    }
}

fn origin_of(page: &str) -> Option<String> {
    let u = url::Url::parse(page).ok()?;
    u.origin().is_tuple().then(|| u.origin().ascii_serialization())
}

struct Run<'a> {
    ctx: &'a Ctx,
    key: &'a SecretString,
    api: &'a Api,
    label: &'a str,
    devices: &'a [FormFactor],
    origins: Origins,
}

impl Run<'_> {
    async fn page(&self, item: &Item) -> Attempt<Vec<(FormFactor, Found)>> {
        let origin = origin_of(&item.url);
        let mut found = Vec::new();
        let mut error = None;
        let (mut attempts, mut requests) = (0, 0);
        for &device in self.devices {
            let (r, a, n) = lookup(self.ctx, self.key, self.api, self.label, "url", &item.url, device).await;
            attempts += a;
            requests += n;
            let own = match r {
                Ok(v) => v.as_ref().and_then(parse_record),
                Err(e) => {
                    error = Some(e);
                    break;
                }
            };
            if let Some(rec) = own {
                found.push((device, Found::Url(rec)));
                continue;
            }
            let Some(origin) = origin.as_deref() else {
                found.push((device, Found::None));
                continue;
            };
            let slot = self.origins.slot(origin, device);
            let mut known = slot.lock().await;
            if known.is_none() {
                let (r, a, n) =
                    lookup(self.ctx, self.key, self.api, self.label, "origin", origin, device).await;
                attempts += a;
                requests += n;
                match r {
                    Ok(v) => *known = Some(v),
                    Err(e) => {
                        error = Some(e);
                        break;
                    }
                }
            }
            let rec = known.as_ref().and_then(|v| v.as_ref()).and_then(parse_record);
            found.push((device, rec.map_or(Found::None, Found::Origin)));
        }
        Attempt {
            value: (!found.is_empty()).then_some(found),
            error,
            attempts,
            requests,
        }
    }
}

fn rating(metric: Metric, v: Option<f64>) -> Option<&'static str> {
    v.map(|x| rate(metric, x))
}

/// Writes a page's rows and says whether any device had data.
fn save(conn: &Connection, run_id: i64, item: &Item, found: Vec<(FormFactor, Found)>) -> Result<bool> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO cwv_field (url_id, form_factor, run_id, level, lcp_ms, inp_ms, cls, fcp_ms, ttfb_ms,
           lcp_rating, inp_rating, cls_rating, fcp_rating, ttfb_rating, period_start, period_end)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
         ON CONFLICT(url_id, form_factor) DO UPDATE SET run_id = excluded.run_id, level = excluded.level,
           lcp_ms = excluded.lcp_ms, inp_ms = excluded.inp_ms, cls = excluded.cls, fcp_ms = excluded.fcp_ms,
           ttfb_ms = excluded.ttfb_ms, lcp_rating = excluded.lcp_rating, inp_rating = excluded.inp_rating,
           cls_rating = excluded.cls_rating, fcp_rating = excluded.fcp_rating,
           ttfb_rating = excluded.ttfb_rating, period_start = excluded.period_start,
           period_end = excluded.period_end",
    )?;
    let mut any = false;
    for (device, f) in found {
        let (level, rec) = match f {
            Found::Url(r) => ("url", r),
            Found::Origin(r) => ("origin", r),
            Found::None => ("none", Record::default()),
        };
        any |= level != "none";
        stmt.execute(params![
            item.url_id,
            device.id(),
            run_id,
            level,
            rec.lcp_ms,
            rec.inp_ms,
            rec.cls,
            rec.fcp_ms,
            rec.ttfb_ms,
            rating(Metric::Lcp, rec.lcp_ms.map(|x| x as f64)),
            rating(Metric::Inp, rec.inp_ms.map(|x| x as f64)),
            rating(Metric::Cls, rec.cls),
            rating(Metric::Fcp, rec.fcp_ms.map(|x| x as f64)),
            rating(Metric::Ttfb, rec.ttfb_ms.map(|x| x as f64)),
            rec.period_start,
            rec.period_end
        ])?;
    }
    Ok(any)
}

/// Requests a run sends at most: one per page and device, plus one per
/// origin and device for pages without page-level data (those are cached
/// and shared, so they are not counted here).
pub fn estimate(pages: usize, devices: usize, limits: &crate::limits::Limits) -> PerfEstimate {
    PerfEstimate::new(pages, devices, limits, 1)
}

/// Starts a field data run, or resumes `resume` from its queue (FR-005).
#[allow(clippy::too_many_arguments)]
pub async fn run_field_data(
    ctx: &Ctx,
    key: &SecretString,
    api: &Api,
    update: &Update,
    label: &str,
    req: &FieldRequest,
    resume: Option<i64>,
    control: &Control,
    on_progress: &mut dyn FnMut(&RunProgress),
) -> Result<RunProgress> {
    if req.devices.is_empty() {
        return Err(Error::Invalid("choose phone, desktop or both".into()));
    }
    let conn = update.conn();
    let params_json = serde_json::to_value(req).expect("serializes");
    let (run_id, counts) = begin(
        conn,
        Provider::FieldData,
        label,
        &params_json,
        |c| Ok(select_urls(c, &req.urls, req.max)?.ids),
        resume,
    )?;
    let run = Run {
        ctx,
        key,
        api,
        label,
        devices: &req.devices,
        origins: Origins::default(),
    };
    let driver = Driver {
        ctx,
        conn,
        run_id,
        control,
        allowance: None::<&Allowance>,
    };
    let fetch = |item: Item| {
        let run = &run;
        async move { run.page(&item).await }
    };
    driver.run(counts, on_progress, fetch, save).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratings_follow_the_core_web_vitals_limits() {
        assert_eq!(rate(Metric::Lcp, 2500.0), "good");
        assert_eq!(rate(Metric::Lcp, 2501.0), "needs_improvement");
        assert_eq!(rate(Metric::Lcp, 4001.0), "poor");
        assert_eq!(rate(Metric::Inp, 200.0), "good");
        assert_eq!(rate(Metric::Inp, 501.0), "poor");
        assert_eq!(rate(Metric::Cls, 0.1), "good");
        assert_eq!(rate(Metric::Cls, 0.26), "poor");
        assert_eq!(rate(Metric::Fcp, 3000.0), "needs_improvement");
        assert_eq!(rate(Metric::Ttfb, 1801.0), "poor");
    }

    #[test]
    fn reads_a_record_with_a_string_cls_and_the_collection_period() {
        let v = json!({"record": {
            "key": {"url": "https://a.test/", "formFactor": "PHONE"},
            "metrics": {
                "largest_contentful_paint": {"percentiles": {"p75": 2100}},
                "interaction_to_next_paint": {"percentiles": {"p75": 180}},
                "cumulative_layout_shift": {"percentiles": {"p75": "0.05"}},
                "first_contentful_paint": {"percentiles": {"p75": 1500}},
                "experimental_time_to_first_byte": {"percentiles": {"p75": 600}}
            },
            "collectionPeriod": {
                "firstDate": {"year": 2026, "month": 9, "day": 14},
                "lastDate": {"year": 2026, "month": 10, "day": 11}
            }
        }});
        let r = parse_record(&v).unwrap();
        assert_eq!(r.lcp_ms, Some(2100));
        assert_eq!(r.inp_ms, Some(180));
        assert_eq!(r.cls, Some(0.05));
        assert_eq!(r.ttfb_ms, Some(600));
        assert_eq!(r.period_start.as_deref(), Some("2026-09-14"));
        assert_eq!(r.period_end.as_deref(), Some("2026-10-11"));
        assert_eq!(parse_record(&json!({"record": {"metrics": {}}})), None);
    }

    #[test]
    fn origins_are_scheme_host_and_port() {
        assert_eq!(
            origin_of("https://a.test/x?y=1").as_deref(),
            Some("https://a.test")
        );
        assert_eq!(
            origin_of("http://127.0.0.1:8080/p").as_deref(),
            Some("http://127.0.0.1:8080")
        );
        assert_eq!(origin_of("not a url"), None);
    }

    #[test]
    fn one_thousand_pages_on_two_devices_fit_in_fifteen_minutes() {
        let limits = crate::limits::Limits::defaults(Provider::FieldData);
        let e = estimate(1000, 2, &limits);
        assert_eq!(e.requests, 2000);
        assert!(e.duration_secs <= 15 * 60, "{} s", e.duration_secs);
    }
}
