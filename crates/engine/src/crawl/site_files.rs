//! Per-host files read by the AI search checks (spec 015 FR-010, plan D3):
//! `/llms.txt`, `/llms-full.txt` and `/.well-known/tdmrep.json`.
//!
//! Fetched once per internal host after the last page, so they never take a
//! slot a page is waiting for. Bodies are kept, compressed, up to 1 MB.

use reqwest::Client;
use url::Url;

use crate::extract::body;
use crate::fetch::{BodyPolicy, OnLimit, get_following};

pub const PATHS: [&str; 3] = ["/llms.txt", "/llms-full.txt", "/.well-known/tdmrep.json"];

/// The agent entry points of spec 016 (plan D4), fetched the same way when
/// `agents.entry_points` is on: AI Catalog, API catalog (RFC 9727) and the
/// A2A agent card.
pub const AGENT_PATHS: [&str; 3] = [
    "/.well-known/ai-catalog.json",
    "/.well-known/api-catalog",
    "/.well-known/agent-card.json",
];

/// The site files that can list MCP endpoints (spec 016 FR-011).
pub const MCP_CARD_PATHS: [&str; 2] = ["/.well-known/ai-catalog.json", "/.well-known/agent-card.json"];

/// Bodies above this are cut and marked `too_large`.
pub const MAX_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteFileRow {
    pub host_key: String,
    pub path: String,
    pub status: Option<u16>,
    pub final_url: Option<String>,
    pub content_type: Option<String>,
    /// Bytes kept (the stored body's length before compression).
    pub size: Option<i64>,
    /// zstd, as for page bodies.
    pub body: Option<Vec<u8>>,
    /// `ok`, `not_found`, `error`, `too_large`, `skipped`.
    pub outcome: &'static str,
    pub error: Option<String>,
    pub fetched_at: Option<i64>,
}

impl SiteFileRow {
    pub fn skipped(host_key: &str, path: &str, why: &str) -> Self {
        Self {
            host_key: host_key.into(),
            path: path.into(),
            status: None,
            final_url: None,
            content_type: None,
            size: None,
            body: None,
            outcome: "skipped",
            error: Some(why.into()),
            fetched_at: None,
        }
    }
}

pub async fn fetch(client: &Client, host_key: &str, path: &str) -> SiteFileRow {
    let mut row = SiteFileRow::skipped(host_key, path, "");
    row.error = None;
    let Ok(url) = Url::parse(&format!("{host_key}{path}")) else {
        row.outcome = "error";
        row.error = Some("not a valid address".into());
        return row;
    };
    let keep_all = |_: Option<&str>| true;
    let policy = BodyPolicy {
        max_bytes: MAX_BYTES + 1,
        on_limit: OnLimit::Truncate,
        keep: &keep_all,
    };
    let outcome = get_following(client, &url, 5, &policy, &[]).await;
    row.fetched_at = Some(jiff::Timestamp::now().as_millisecond());
    match outcome.result {
        Err(e) => {
            row.outcome = "error";
            row.error = Some(format!("{}: {}", e.class, e.message));
        }
        Ok(f) => {
            let status = f.status.as_u16();
            row.status = Some(status);
            row.final_url = Some(f.final_url.to_string());
            row.content_type = f.content_type;
            if !f.status.is_success() {
                row.outcome = if (400..500).contains(&status) {
                    "not_found"
                } else {
                    "error"
                };
                return row;
            }
            let mut bytes = f.body.unwrap_or_default();
            row.outcome = "ok";
            if bytes.len() as u64 > MAX_BYTES {
                bytes.truncate(MAX_BYTES as usize);
                row.outcome = "too_large";
            }
            row.size = Some(bytes.len() as i64);
            match body::compress(&bytes) {
                Ok(packed) => row.body = Some(packed),
                Err(e) => {
                    row.outcome = "error";
                    row.error = Some(format!("could not compress: {e}"));
                }
            }
        }
    }
    row
}
