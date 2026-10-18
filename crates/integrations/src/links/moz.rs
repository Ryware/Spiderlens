//! Moz Links API v2, `url_metrics` (spec 024 US5, plan D10).
//!
//! One request carries up to 50 targets and uses one row per target. The
//! credential is the Access ID and Secret Key joined by a colon
//! (`ACCESS_ID:SECRET_KEY`), saved as one keychain entry.

use reqwest::Method;
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};

use super::{BatchOut, MetricDef, pick};
use crate::http::HttpError;
use crate::run::Ctx;

/// Targets per `url_metrics` request.
pub const BATCH: usize = 50;

pub const METRICS: &[MetricDef] = &[
    MetricDef {
        id: "page_authority",
        label: "Page Authority",
        field: "page_authority",
    },
    MetricDef {
        id: "domain_authority",
        label: "Domain Authority",
        field: "domain_authority",
    },
    MetricDef {
        id: "linking_domains",
        label: "Linking domains",
        field: "root_domains_to_page",
    },
    MetricDef {
        id: "backlinks",
        label: "Backlinks",
        field: "pages_to_page",
    },
    MetricDef {
        id: "spam_score",
        label: "Spam Score",
        field: "spam_score",
    },
];

/// Looks up `urls` (at most [`BATCH`]) and returns one row per URL, in order.
pub async fn fetch(
    ctx: &Ctx,
    credential: &SecretString,
    base: &str,
    urls: &[&str],
    defs: &[MetricDef],
) -> Result<BatchOut, HttpError> {
    let secret = credential.expose_secret();
    let Some((id, key)) = secret.split_once(':') else {
        return Err(HttpError::Unauthorized {
            provider: "Moz".into(),
            message: "the saved key should be ACCESS_ID:SECRET_KEY; save it again".into(),
        });
    };
    let req = ctx
        .http
        .request(Method::POST, &format!("{base}/url_metrics"))
        .basic_auth(id, Some(key))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(json!({"targets": urls}).to_string());
    let (v, _) = ctx.http.json_with_headers("Moz", req).await?;
    let mut rows = vec![None; urls.len()];
    let list: Vec<Value> = v["results"].as_array().cloned().unwrap_or_default();
    for (i, r) in list.iter().enumerate() {
        if i < rows.len() {
            rows[i] = Some(pick(r, defs));
        }
    }
    // One row per target.
    Ok(BatchOut {
        rows,
        units: Some(urls.len() as u64),
    })
}
