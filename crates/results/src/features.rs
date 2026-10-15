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
    /// Link analysis settings the crawl was made with (spec 023); the app
    /// can replace them with `Reader::set_link_settings`.
    #[serde(skip)]
    #[ts(skip)]
    pub link_settings: spiderlens_config::LinkAnalysisSettings,
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
        // Integration data (spec 024) is listed under its own key, written
        // by the integrations crate, not by the crawl.
        let integrations = meta("integration_features")?.unwrap_or_default();
        let mut available = vec![Need::Base];
        for need in [
            Need::Extraction,
            Need::Attempts,
            Need::Timing,
            Need::RawHtml,
            Need::Rendering,
            Need::RenderedHtml,
            Need::Screenshots,
            Need::Agents,
            Need::Amp,
            Need::Pdf,
            Need::Readability,
            Need::SitemapEntries,
        ] {
            if listed.split(',').any(|f| f.trim() == need.feature_id()) {
                available.push(need);
            }
        }
        for need in [Need::SearchConsole] {
            if integrations.split(',').any(|f| f.trim() == need.feature_id()) {
                available.push(need);
            }
        }
        // Crawl Analysis runs once fetching has ended (spec 023 FR-029).
        if meta("status")?.as_deref() != Some("running") {
            available.push(Need::Analysis);
        }
        // Spec 015: the AI checks record their run in the meta table.
        if meta("aeo_analysis")?.is_some() {
            available.push(Need::AiChecks);
        }
        // The cookie audit arrived with a crawl file format step, so the
        // table says it all, also for older crawls resumed after upgrading.
        let cookies: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'cookies')",
            [],
            |r| r.get(0),
        )?;
        if cookies {
            available.push(Need::Cookies);
        }
        // Written by the spelling pass, not the crawl (spec 027 part D).
        if meta("spelling")?.is_some() {
            available.push(Need::Spelling);
        }
        let redirect_limit = config
            .as_ref()
            .and_then(|v| find_u64(v, "max_redirects"))
            .map_or(10, |n| n as u32);
        let link_settings = stored
            .as_ref()
            .map(|s| s.config.links.clone())
            .unwrap_or_default();
        let robots_token = stored
            .and_then(|s| s.config.identity.user_agent.token())
            .unwrap_or_else(|| "SpiderLens".into())
            .to_lowercase();
        Ok(Self {
            available,
            redirect_limit,
            robots_token,
            link_settings,
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
