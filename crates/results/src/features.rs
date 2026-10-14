//! What data an opened crawl file holds (plan D3).
//!
//! Base data is in every crawl file. Later specs add more and say so in the
//! `data_features` meta value (a comma-separated list of [`Need`] ids), so a
//! file made before them shows those columns as "not collected" instead of
//! failing (Edge case: older crawl files).
//!
//! Files written by spec 005 before the engine listed its features carry
//! `extraction_version` instead; they hold page content and timing, and the
//! raw HTML when the crawl's settings stored it.

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use spiderlens_config::StoredCrawlConfig;
use ts_rs::TS;

use crate::tabs::Need;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Features {
    pub available: Vec<Need>,
    /// The crawl's redirect hop limit, for the "chain over limit" filter.
    pub redirect_limit: u32,
    /// The token robots directives are matched against, lowercase.
    #[serde(skip)]
    #[ts(skip)]
    pub robots_token: String,
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
        let config = meta("config")?.and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok());
        let stored = config
            .clone()
            .and_then(|v| serde_json::from_value::<StoredCrawlConfig>(v).ok());
        let listed = match meta("data_features")? {
            Some(l) => l,
            None if meta("extraction_version")?.is_some() => {
                let html = stored.as_ref().is_none_or(|s| s.config.extraction.store_html);
                format!("extraction,timing{}", if html { ",raw_html" } else { "" })
            }
            None => String::new(),
        };
        let mut available = vec![Need::Base];
        for need in [Need::Extraction, Need::Attempts, Need::Timing, Need::RawHtml] {
            if listed.split(',').any(|f| f.trim() == need.feature_id()) {
                available.push(need);
            }
        }
        // Written by the spelling pass, not the crawl (spec 027 part D).
        if meta("spelling")?.is_some() {
            available.push(Need::Spelling);
        }
        let redirect_limit = config
            .as_ref()
            .and_then(|v| find_u64(v, "max_redirects"))
            .map_or(10, |n| n as u32);
        let robots_token = stored
            .and_then(|s| s.config.identity.user_agent.token())
            .unwrap_or_else(|| "SpiderLens".into())
            .to_lowercase();
        Ok(Self {
            available,
            redirect_limit,
            robots_token,
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
