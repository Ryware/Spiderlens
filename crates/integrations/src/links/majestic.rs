//! Majestic API, `GetIndexItemInfo` (spec 024 US5, plan D10).
//!
//! One request carries up to 100 items and uses one unit per item. Majestic
//! reports failures inside a 200 answer (`Code` other than `OK`), and takes
//! the key as a query parameter, so errors never print the request URL.

use reqwest::Method;
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;

use super::{BatchOut, MetricDef, pick};
use crate::http::HttpError;
use crate::run::Ctx;

/// Items per `GetIndexItemInfo` request.
pub const BATCH: usize = 100;

pub const METRICS: &[MetricDef] = &[
    MetricDef {
        id: "trust_flow",
        label: "Trust Flow",
        field: "TrustFlow",
    },
    MetricDef {
        id: "citation_flow",
        label: "Citation Flow",
        field: "CitationFlow",
    },
    MetricDef {
        id: "backlinks",
        label: "Backlinks",
        field: "ExtBackLinks",
    },
    MetricDef {
        id: "refdomains",
        label: "Referring domains",
        field: "RefDomains",
    },
];

/// Maps Majestic's in-body error codes to the shared error classes.
fn failure(code: &str, message: &str) -> HttpError {
    let provider = "Majestic".to_string();
    let message = if message.is_empty() {
        code.to_string()
    } else {
        format!("{code}: {message}")
    };
    let lower = code.to_ascii_lowercase();
    if lower.contains("insufficient") || lower.contains("units") || lower.contains("limit") {
        HttpError::RateLimited {
            provider,
            retry_after: None,
            quota: true,
            message,
        }
    } else if lower.contains("key") || lower.contains("access") || lower.contains("auth") {
        HttpError::Unauthorized { provider, message }
    } else {
        HttpError::Client {
            provider,
            status: 200,
            message,
        }
    }
}

/// Looks up `urls` (at most [`BATCH`]) and returns one row per URL, in order.
pub async fn fetch(
    ctx: &Ctx,
    key: &SecretString,
    base: &str,
    urls: &[&str],
    defs: &[MetricDef],
) -> Result<BatchOut, HttpError> {
    let mut query: Vec<(String, String)> = vec![
        ("app_api_key".into(), key.expose_secret().to_string()),
        ("cmd".into(), "GetIndexItemInfo".into()),
        ("items".into(), urls.len().to_string()),
        ("datasource".into(), "fresh".into()),
    ];
    for (i, u) in urls.iter().enumerate() {
        query.push((format!("item{i}"), (*u).to_string()));
    }
    let req = ctx.http.request(Method::GET, base).query(&query);
    let (v, _) = ctx.http.json_with_headers("Majestic", req).await?;
    let code = v["Code"].as_str().unwrap_or_default();
    if code != "OK" {
        return Err(failure(code, v["ErrorMessage"].as_str().unwrap_or_default()));
    }
    let mut rows = vec![None; urls.len()];
    let data: Vec<Value> = v
        .pointer("/DataTables/Results/Data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for (i, d) in data.iter().enumerate() {
        let at = d["ItemNum"].as_u64().map_or(i, |n| n as usize);
        if at < rows.len() {
            // An item Majestic has not seen has no numbers.
            rows[at] = Some(if d["ResultCode"].as_str().is_none_or(|c| c == "OK") {
                pick(d, defs)
            } else {
                serde_json::Map::new()
            });
        }
    }
    // One unit per item looked up.
    Ok(BatchOut {
        rows,
        units: Some(urls.len() as u64),
    })
}
