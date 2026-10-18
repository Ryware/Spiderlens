//! Ahrefs API v3, Batch Analysis (spec 024 US5, plan D10).
//!
//! One request carries up to 100 targets. Ahrefs bills API units per
//! request: a minimum per request plus an amount per row and field (the
//! minimum is in research.md). The estimate uses those two numbers; the run
//! re-estimates from the units the response reports.

use reqwest::Method;
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};

use super::{BatchOut, MetricDef, header_u64, pick};
use crate::http::HttpError;
use crate::run::Ctx;

/// Targets per Batch Analysis request.
pub const BATCH: usize = 100;

/// Units every request costs at least (research.md).
pub const MIN_REQUEST_UNITS: u64 = 50;

pub const METRICS: &[MetricDef] = &[
    MetricDef {
        id: "domain_rating",
        label: "Domain Rating",
        field: "domain_rating",
    },
    MetricDef {
        id: "url_rating",
        label: "URL Rating",
        field: "url_rating",
    },
    MetricDef {
        id: "backlinks",
        label: "Backlinks",
        field: "backlinks",
    },
    MetricDef {
        id: "refdomains",
        label: "Referring domains",
        field: "refdomains",
    },
];

/// Looks up `urls` (at most [`BATCH`]) and returns one row per URL, in order.
pub async fn fetch(
    ctx: &Ctx,
    key: &SecretString,
    base: &str,
    urls: &[&str],
    defs: &[MetricDef],
) -> Result<BatchOut, HttpError> {
    let body = json!({
        "select": defs.iter().map(|d| d.field).collect::<Vec<_>>(),
        "targets": urls
            .iter()
            .map(|u| json!({"url": u, "mode": "exact", "protocol": "both"}))
            .collect::<Vec<_>>(),
    });
    let req = ctx
        .http
        .request(Method::POST, &format!("{base}/batch-analysis"))
        .bearer_auth(key.expose_secret())
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body.to_string());
    let (v, headers) = ctx.http.json_with_headers("Ahrefs", req).await?;
    let mut rows = vec![None; urls.len()];
    let list: Vec<Value> = v["targets"].as_array().cloned().unwrap_or_default();
    for (i, t) in list.iter().enumerate() {
        let at = t["index"].as_u64().map_or(i, |n| n as usize);
        if at < rows.len() {
            rows[at] = Some(pick(t, defs));
        }
    }
    Ok(BatchOut {
        rows,
        units: header_u64(&headers, "x-api-units-cost-total-actual")
            .or_else(|| header_u64(&headers, "x-api-units-cost-total")),
    })
}
