//! PageSpeed Insights lab tests (spec 024 US3, plan D10).
//!
//! `runPagespeed` with all four categories and the chosen strategy. Each
//! call runs Lighthouse remotely and is slow, so a run keeps a handful in
//! flight (the provider's concurrency), stays under the per-100-second rate,
//! and counts every call against the daily allowance. What is stored per page
//! and strategy: the category scores, the lab metrics, the top five
//! opportunities, and the accessibility, best-practices and SEO audits
//! without screenshots (zstd JSON) for spec 026. The API key travels in the
//! `x-goog-api-key` header.

use reqwest::Method;
use rusqlite::{Connection, params};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use spiderlens_engine::crawl::Update;
use ts_rs::TS;

use crate::http::HttpError;
use crate::limits::{Limits, with_retry};
use crate::perf::{Allowance, Attempt, Driver, Item, PerfEstimate, UrlSet, begin, select_urls};
use crate::run::{Control, Ctx};
use crate::store::RunProgress;
use crate::{Provider, Result, cache};

/// How long one Lighthouse run takes to answer, for the estimate. Pages vary
/// between about ten and forty seconds.
pub const SECONDS_PER_TEST: u64 = 25;

#[derive(Debug, Clone)]
pub struct Api {
    pub base: String,
}

impl Default for Api {
    fn default() -> Self {
        Self {
            base: "https://www.googleapis.com/pagespeedonline/v5".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Strategy {
    Mobile,
    Desktop,
}

impl Strategy {
    pub fn id(self) -> &'static str {
        match self {
            Self::Mobile => "mobile",
            Self::Desktop => "desktop",
        }
    }
}

/// The device choice of a lab run (US3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum StrategyChoice {
    Mobile,
    Desktop,
    Both,
}

impl StrategyChoice {
    pub fn strategies(self) -> Vec<Strategy> {
        match self {
            Self::Mobile => vec![Strategy::Mobile],
            Self::Desktop => vec![Strategy::Desktop],
            Self::Both => vec![Strategy::Mobile, Strategy::Desktop],
        }
    }
}

/// What a lab test run asks for (FR-026).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LabRequest {
    pub strategy: StrategyChoice,
    pub urls: UrlSet,
    /// Keep only the first pages (shallowest first).
    pub max: Option<u32>,
    /// Ignore cached answers.
    #[serde(default)]
    pub fresh: bool,
}

impl Default for LabRequest {
    /// Lab tests are slow, so the default is a small set of indexable pages.
    fn default() -> Self {
        Self {
            strategy: StrategyChoice::Mobile,
            urls: UrlSet::Indexable,
            max: Some(50),
            fresh: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Opportunity {
    pub id: String,
    pub title: String,
    pub savings_ms: i64,
}

/// One page's lab result for one strategy, as stored (and cached).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lab {
    pub performance: Option<i64>,
    pub accessibility: Option<i64>,
    pub best_practices: Option<i64>,
    pub seo: Option<i64>,
    pub lcp_ms: Option<i64>,
    pub cls: Option<f64>,
    pub tbt_ms: Option<i64>,
    pub speed_index_ms: Option<i64>,
    pub opportunities: Vec<Opportunity>,
    pub audits: Vec<Value>,
}

const CATEGORIES: [&str; 4] = ["PERFORMANCE", "ACCESSIBILITY", "BEST_PRACTICES", "SEO"];
/// Categories whose audits are kept for spec 026.
const KEPT: [&str; 3] = ["accessibility", "best-practices", "seo"];
const AUDIT_ITEMS: usize = 25;

fn score(lh: &Value, category: &str) -> Option<i64> {
    lh.pointer(&format!("/categories/{category}/score"))
        .and_then(Value::as_f64)
        .map(|s| (s * 100.0).round() as i64)
}

fn numeric(lh: &Value, audit: &str) -> Option<f64> {
    lh.pointer(&format!("/audits/{audit}/numericValue")).and_then(Value::as_f64)
}

/// An audit's table without what is not needed later: debug data, and
/// anything beyond the first rows.
fn trim_details(d: &Value) -> Option<Value> {
    let kind = d["type"].as_str()?;
    if matches!(kind, "screenshot" | "filmstrip" | "debugdata") {
        return None;
    }
    let mut out = serde_json::Map::new();
    out.insert("type".into(), json!(kind));
    if let Some(h) = d.get("headings") {
        out.insert("headings".into(), h.clone());
    }
    if let Some(items) = d["items"].as_array() {
        out.insert("items".into(), Value::Array(items.iter().take(AUDIT_ITEMS).cloned().collect()));
    }
    Some(Value::Object(out))
}

fn kept_audits(lh: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    for category in KEPT {
        let refs = lh["categories"][category]["auditRefs"].as_array().into_iter().flatten();
        for r in refs {
            let Some(id) = r["id"].as_str() else {
                continue;
            };
            let a = &lh["audits"][id];
            if a.is_null() {
                continue;
            }
            let passed = a["score"].as_f64() == Some(1.0);
            // Rows of the audits that did not pass: the elements to fix.
            let details = if passed {
                Value::Null
            } else {
                trim_details(&a["details"]).unwrap_or(Value::Null)
            };
            out.push(json!({
                "category": category,
                "id": id,
                "title": a["title"],
                "score": a["score"],
                "mode": a["scoreDisplayMode"],
                "display": a["displayValue"],
                "weight": r["weight"],
                "details": details,
            }));
        }
    }
    out
}

fn top_opportunities(lh: &Value) -> Vec<Opportunity> {
    let mut all: Vec<Opportunity> = lh["audits"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, a)| a.pointer("/details/type").and_then(Value::as_str) == Some("opportunity"))
        .filter_map(|(id, a)| {
            let ms = a.pointer("/details/overallSavingsMs").and_then(Value::as_f64)?;
            (ms > 0.0).then(|| Opportunity {
                id: id.clone(),
                title: a["title"].as_str().unwrap_or(id).to_string(),
                savings_ms: ms.round() as i64,
            })
        })
        .collect();
    all.sort_by(|a, b| b.savings_ms.cmp(&a.savings_ms).then_with(|| a.id.cmp(&b.id)));
    all.truncate(5);
    all
}

/// Reads a `runPagespeed` answer; `None` when Lighthouse returned nothing.
pub fn parse_lab(v: &Value) -> Option<Lab> {
    let lh = v.get("lighthouseResult").filter(|l| l.get("categories").is_some())?;
    Some(Lab {
        performance: score(lh, "performance"),
        accessibility: score(lh, "accessibility"),
        best_practices: score(lh, "best-practices"),
        seo: score(lh, "seo"),
        lcp_ms: numeric(lh, "largest-contentful-paint").map(|x| x.round() as i64),
        cls: numeric(lh, "cumulative-layout-shift"),
        tbt_ms: numeric(lh, "total-blocking-time").map(|x| x.round() as i64),
        speed_index_ms: numeric(lh, "speed-index").map(|x| x.round() as i64),
        opportunities: top_opportunities(lh),
        audits: kept_audits(lh),
    })
}

/// Asks for one page and strategy, from the cache or the API. Returns the
/// result, the attempts, and the requests sent.
async fn test_page(
    ctx: &Ctx,
    key: &SecretString,
    api: &Api,
    label: &str,
    url: &str,
    strategy: Strategy,
) -> (std::result::Result<Lab, HttpError>, u32, u64) {
    // The cache holds the compact result, not Lighthouse's large answer.
    let ck = cache::key(&["lab_tests", label, url, strategy.id()]);
    if let Some(lab) = ctx.cached(&ck).and_then(|v| serde_json::from_value::<Lab>(v).ok()) {
        return (Ok(lab), 0, 0);
    }
    let endpoint = format!("{}/runPagespeed", api.base);
    let mut query = vec![("url", url), ("strategy", strategy.id())];
    query.extend(CATEGORIES.iter().map(|c| ("category", *c)));
    let (r, attempts) = with_retry(|| async {
        ctx.bucket.acquire().await;
        let req = ctx
            .http
            .request(Method::GET, &endpoint)
            .header("x-goog-api-key", key.expose_secret())
            .query(&query);
        ctx.http.json("PageSpeed Insights", req).await
    })
    .await;
    let sent = u64::from(attempts);
    let r = r.and_then(|v| {
        parse_lab(&v).ok_or_else(|| HttpError::Decode {
            provider: "PageSpeed Insights".into(),
            message: "the answer holds no Lighthouse result".into(),
        })
    });
    if let Ok(lab) = &r {
        ctx.remember(&ck, &serde_json::to_value(lab).expect("serializes"));
    }
    (r, attempts, sent)
}

async fn page(
    ctx: &Ctx,
    key: &SecretString,
    api: &Api,
    label: &str,
    strategies: &[Strategy],
    item: &Item,
) -> Attempt<Vec<(Strategy, Lab)>> {
    let mut labs = Vec::new();
    let mut error = None;
    let (mut attempts, mut requests) = (0, 0);
    for &s in strategies {
        let (r, a, n) = test_page(ctx, key, api, label, &item.url, s).await;
        attempts += a;
        requests += n;
        match r {
            Ok(lab) => labs.push((s, lab)),
            Err(e) => {
                error = Some(e);
                break;
            }
        }
    }
    Attempt {
        value: (!labs.is_empty()).then_some(labs),
        error,
        attempts,
        requests,
    }
}

fn save(conn: &Connection, run_id: i64, item: &Item, labs: Vec<(Strategy, Lab)>) -> Result<bool> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO psi_lab (url_id, strategy, run_id, performance, accessibility, best_practices, seo,
           lcp_ms, cls, tbt_ms, speed_index_ms, opportunities, audits, tested_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
         ON CONFLICT(url_id, strategy) DO UPDATE SET run_id = excluded.run_id,
           performance = excluded.performance, accessibility = excluded.accessibility,
           best_practices = excluded.best_practices, seo = excluded.seo, lcp_ms = excluded.lcp_ms,
           cls = excluded.cls, tbt_ms = excluded.tbt_ms, speed_index_ms = excluded.speed_index_ms,
           opportunities = excluded.opportunities, audits = excluded.audits, tested_at = excluded.tested_at",
    )?;
    let now = jiff::Timestamp::now().as_second();
    for (strategy, lab) in &labs {
        let opportunities = serde_json::to_string(&lab.opportunities).expect("serializes");
        let audits = zstd::encode_all(&serde_json::to_vec(&lab.audits).expect("serializes")[..], 3)?;
        stmt.execute(params![
            item.url_id,
            strategy.id(),
            run_id,
            lab.performance,
            lab.accessibility,
            lab.best_practices,
            lab.seo,
            lab.lcp_ms,
            lab.cls,
            lab.tbt_ms,
            lab.speed_index_ms,
            opportunities,
            audits,
            now
        ])?;
    }
    Ok(!labs.is_empty())
}

/// What `pages` pages on `strategies` devices cost: the run is as slow as
/// Lighthouse allows with the provider's concurrency (SC-003 covers field
/// data; lab tests take hours for large sets and the panel says so).
pub fn estimate(pages: usize, strategies: usize, limits: &Limits) -> PerfEstimate {
    PerfEstimate::new(pages, strategies, limits, SECONDS_PER_TEST)
}

/// Decodes a stored `psi_lab.audits` blob.
pub fn decode_audits(blob: &[u8]) -> Option<Vec<Value>> {
    let raw = zstd::decode_all(blob).ok()?;
    serde_json::from_slice(&raw).ok()
}

/// Starts a lab test run, or resumes `resume` from its queue (FR-005).
#[allow(clippy::too_many_arguments)]
pub async fn run_lab_tests(
    ctx: &Ctx,
    key: &SecretString,
    api: &Api,
    allowance: Option<&Allowance>,
    update: &Update,
    label: &str,
    req: &LabRequest,
    resume: Option<i64>,
    control: &Control,
    on_progress: &mut dyn FnMut(&RunProgress),
) -> Result<RunProgress> {
    let strategies = req.strategy.strategies();
    let conn = update.conn();
    let params_json = serde_json::to_value(req).expect("serializes");
    let (run_id, counts) = begin(
        conn,
        Provider::LabTests,
        label,
        &params_json,
        |c| Ok(select_urls(c, &req.urls, req.max)?.ids),
        resume,
    )?;
    let driver = Driver {
        ctx,
        conn,
        run_id,
        control,
        allowance,
    };
    let strategies = &strategies;
    let fetch = |item: Item| async move { page(ctx, key, api, label, strategies, &item).await };
    driver.run(counts, on_progress, fetch, save).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer() -> Value {
        json!({"lighthouseResult": {
            "categories": {
                "performance": {"score": 0.874},
                "accessibility": {"score": 0.9, "auditRefs": [{"id": "image-alt", "weight": 10}, {"id": "color-contrast", "weight": 7}]},
                "best-practices": {"score": 1.0, "auditRefs": [{"id": "uses-https", "weight": 5}]},
                "seo": {"score": 0.8, "auditRefs": [{"id": "meta-description", "weight": 3}]}
            },
            "audits": {
                "largest-contentful-paint": {"numericValue": 2412.6},
                "cumulative-layout-shift": {"numericValue": 0.043},
                "total-blocking-time": {"numericValue": 120.2},
                "speed-index": {"numericValue": 3100.0},
                "render-blocking-resources": {"title": "Eliminate render-blocking resources", "details": {"type": "opportunity", "overallSavingsMs": 850.0}},
                "unused-javascript": {"title": "Reduce unused JavaScript", "details": {"type": "opportunity", "overallSavingsMs": 600.0}},
                "offscreen-images": {"title": "Defer offscreen images", "details": {"type": "opportunity", "overallSavingsMs": 0.0}},
                "image-alt": {"title": "Images have alt text", "score": 0, "scoreDisplayMode": "binary",
                    "details": {"type": "table", "headings": [], "items": [{"node": {"snippet": "<img src=a.png>"}}], "debugData": {"big": 1}}},
                "color-contrast": {"title": "Contrast", "score": 1, "scoreDisplayMode": "binary", "details": {"type": "table", "items": [{"x": 1}]}},
                "uses-https": {"title": "Uses HTTPS", "score": 1, "scoreDisplayMode": "binary"},
                "meta-description": {"title": "Has a meta description", "score": 0, "scoreDisplayMode": "binary"}
            }
        }})
    }

    #[test]
    fn reads_scores_metrics_and_the_top_opportunities() {
        let lab = parse_lab(&answer()).unwrap();
        assert_eq!(lab.performance, Some(87));
        assert_eq!(lab.accessibility, Some(90));
        assert_eq!(lab.best_practices, Some(100));
        assert_eq!(lab.seo, Some(80));
        assert_eq!(lab.lcp_ms, Some(2413));
        assert_eq!(lab.cls, Some(0.043));
        assert_eq!(lab.tbt_ms, Some(120));
        assert_eq!(lab.speed_index_ms, Some(3100));
        // Savings of zero are not opportunities; the rest are ordered by savings.
        let ids: Vec<&str> = lab.opportunities.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, ["render-blocking-resources", "unused-javascript"]);
        assert_eq!(parse_lab(&json!({"error": {}})), None);
    }

    #[test]
    fn keeps_the_audits_spec_026_reads_and_drops_debug_data() {
        let lab = parse_lab(&answer()).unwrap();
        assert_eq!(lab.audits.len(), 4);
        let alt = lab.audits.iter().find(|a| a["id"] == "image-alt").unwrap();
        assert_eq!(alt["category"], "accessibility");
        assert_eq!(alt["details"]["items"][0]["node"]["snippet"], "<img src=a.png>");
        assert!(alt["details"].get("debugData").is_none());
        // A passing audit carries no rows.
        let ok = lab.audits.iter().find(|a| a["id"] == "color-contrast").unwrap();
        assert!(ok["details"].is_null());
    }

    #[test]
    fn lab_tests_are_paced_by_lighthouse_not_the_rate() {
        let limits = Limits::defaults(Provider::LabTests);
        let e = estimate(1000, 2, &limits);
        assert_eq!(e.requests, 2000);
        assert!(e.duration_secs > 15 * 60);
        assert_eq!(e.concurrency, 6);
    }

    #[test]
    fn strategy_choices_expand() {
        assert_eq!(StrategyChoice::Both.strategies(), [Strategy::Mobile, Strategy::Desktop]);
        assert_eq!(StrategyChoice::Desktop.strategies(), [Strategy::Desktop]);
    }
}
