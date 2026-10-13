//! What data an opened crawl file holds (plan D3).
//!
//! Base data is in every crawl file. Later specs add more and say so in the
//! `data_features` meta value (a comma-separated list of [`Need`] ids), so a
//! file made before them shows those columns as "not collected" instead of
//! failing (Edge case: older crawl files).

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use ts_rs::TS;

use crate::tabs::Need;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Features {
    pub available: Vec<Need>,
    /// The crawl's redirect hop limit, for the "chain over limit" filter.
    pub redirect_limit: u32,
}

impl Features {
    pub fn has(&self, need: Need) -> bool {
        need == Need::Base || self.available.contains(&need)
    }

    pub(crate) fn detect(conn: &Connection) -> rusqlite::Result<Self> {
        let meta = |key: &str| -> rusqlite::Result<Option<String>> {
            conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
                .optional()
        };
        let listed = meta("data_features")?.unwrap_or_default();
        let mut available = vec![Need::Base];
        for need in [Need::Extraction, Need::Attempts, Need::Timing, Need::RawHtml] {
            if listed.split(',').any(|f| f.trim() == need.feature_id()) {
                available.push(need);
            }
        }
        let redirect_limit = meta("config")?
            .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
            .and_then(|v| find_u64(&v, "max_redirects"))
            .map_or(10, |n| n as u32);
        Ok(Self {
            available,
            redirect_limit,
        })
    }
}

fn find_u64(v: &serde_json::Value, key: &str) -> Option<u64> {
    match v {
        serde_json::Value::Object(m) => m
            .get(key)
            .and_then(serde_json::Value::as_u64)
            .or_else(|| m.values().find_map(|x| find_u64(x, key))),
        _ => None,
    }
}
