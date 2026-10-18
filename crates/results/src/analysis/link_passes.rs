//! The `link_positions` and `link_metrics` Crawl Analysis passes (spec 023
//! stage 2, plan D1 and D6) and the run record they leave in the file.
//!
//! Each pass wraps the stage 1 builder and writes into `cx.schema`. They
//! read the link analysis settings stored with the crawl, like
//! `Features::detect`. The record (`link_analysis_run`, FR-014) is what the
//! reader checks to tell whether the stored tables match the settings it
//! shows (plan D5).

use std::time::{Duration, Instant};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use spiderlens_config::{LinkAnalysisSettings, PositionRule, PositionWeights};

use super::{AnalysisProgress, AnalysisRun, Pass, PassContext, Watch, link_metrics, link_positions};
use crate::features::Features;
use crate::tabs::Need;
use crate::{Error, Result};

/// The two pass ids, in run order.
pub const LINK_PASSES: [&str; 2] = ["link_positions", "link_metrics"];

/// Where each link on its page sits (FR-017 to FR-021).
pub struct LinkPositions;

/// Link Score, ranks and link counts (FR-004 to FR-008).
pub struct LinkMetrics;

impl Pass for LinkPositions {
    fn id(&self) -> &'static str {
        "link_positions"
    }
    fn version(&self) -> u32 {
        1
    }
    fn needs(&self) -> &'static [Need] {
        &[Need::Analysis]
    }
    /// Spec 023 plan D1: 5 s per 100,000 pages.
    fn budget(&self) -> Duration {
        Duration::from_secs(5)
    }
    /// Without stored HTML this still runs: every link is Unknown (US4
    /// scenario 6).
    fn run(&self, cx: &mut PassContext<'_>) -> Result<()> {
        let settings = Features::detect(cx.conn)?.link_settings;
        let (conn, schema) = (cx.conn, cx.schema);
        with_watch(cx, "Link positions", |w| {
            link_positions::build(conn, schema, &settings.position_rules, w).map(|_| ())
        })
    }
}

impl Pass for LinkMetrics {
    fn id(&self) -> &'static str {
        "link_metrics"
    }
    fn version(&self) -> u32 {
        1
    }
    fn after(&self) -> &'static [&'static str] {
        &["link_positions"]
    }
    fn needs(&self) -> &'static [Need] {
        &[Need::Analysis]
    }
    /// Spec 023 SC-002: 10 s per 100,000 pages.
    fn budget(&self) -> Duration {
        Duration::from_secs(10)
    }
    fn run(&self, cx: &mut PassContext<'_>) -> Result<()> {
        let t0 = Instant::now();
        let settings = Features::detect(cx.conn)?.link_settings;
        let (conn, schema) = (cx.conn, cx.schema);
        let mut stats = None;
        with_watch(cx, "Link Score", |w| {
            stats = Some(link_metrics::build(conn, schema, settings.weighting.as_ref(), w)?);
            Ok(())
        })?;
        let stats = stats.expect("built");
        if schema != "main" {
            return Ok(());
        }
        // The positions pass ran just before in the same run; its time is
        // in the `analysis` record.
        let positions_ms = super::records(conn)?.get("link_positions").map_or(0, |r| r.ms);
        let links: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {schema}.link_positions"),
            [],
            |r| r.get(0),
        )?;
        let without_html: i64 = conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM urls u WHERE {} AND NOT EXISTS (
                   SELECT 1 FROM bodies b WHERE b.url_id = u.id AND b.body IS NOT NULL
                     AND b.codec = '{}')",
                crate::tabs::HTML_OK,
                spiderlens_engine::extract::body::CODEC
            ),
            [],
            |r| r.get(0),
        )?;
        let complete: bool = conn
            .query_row("SELECT value FROM meta WHERE key = 'status'", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
            .is_some_and(|s| s == "completed");
        let run = StoredRun {
            ran_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as i64),
            app_version: spiderlens_engine::crawl::store::APP_VERSION.into(),
            crawl_complete: complete,
            urls: stats.urls,
            links: links as u64,
            rounds: stats.rounds,
            weighting: settings.weighting,
            rules: settings.position_rules,
            ms: (positions_ms + t0.elapsed().as_millis() as u64).min(u64::from(u32::MAX)) as u32,
            pages_without_html: without_html as u64,
        };
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('link_analysis_run', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [serde_json::to_string(&run).expect("run serializes")],
        )?;
        Ok(())
    }
}

/// Runs `f` with a [`Watch`] that reports through the pass's progress
/// channel. A cancel surfaces as [`Error::Cancelled`].
fn with_watch(
    cx: &mut PassContext<'_>,
    label: &str,
    f: impl FnOnce(&mut Watch<'_>) -> rusqlite::Result<()>,
) -> Result<()> {
    let cancel = cx.cancel;
    let mut on = |p: AnalysisProgress| {
        // The watch itself stops at the next tick once cancel is set.
        let _ = cx.progress(p.done, p.total, label);
    };
    let mut watch = Watch::new(&mut on, cancel);
    match f(&mut watch) {
        Ok(()) => Ok(()),
        Err(_) if watch.cancelled() => Err(Error::Cancelled),
        Err(e) => Err(e.into()),
    }
}

/// `link_analysis_run` in the meta table (data-model.md).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredRun {
    /// Unix milliseconds.
    pub ran_at: i64,
    pub app_version: String,
    /// The crawl had finished (not paused or stopped) when it ran.
    pub crawl_complete: bool,
    pub urls: u64,
    pub links: u64,
    pub rounds: u32,
    pub weighting: Option<PositionWeights>,
    pub rules: Vec<PositionRule>,
    #[serde(default)]
    pub ms: u32,
    #[serde(default)]
    pub pages_without_html: u64,
}

impl From<StoredRun> for AnalysisRun {
    fn from(r: StoredRun) -> Self {
        Self {
            urls: r.urls,
            links: r.links,
            rounds: r.rounds,
            pages_without_html: r.pages_without_html,
            weighted: r.weighting.is_some(),
            ms: r.ms,
        }
    }
}

/// The run stored in the file when both passes ran on this crawl data with
/// this build's versions and with exactly `settings` (plan D5). None means
/// the reader has to work the tables out itself.
pub(crate) fn stored_run(conn: &Connection, settings: &LinkAnalysisSettings) -> Result<Option<StoredRun>> {
    let recs = super::records(conn)?;
    let stamp = super::data_stamp(conn)?;
    let passes: [&dyn Pass; 2] = [&LinkPositions, &LinkMetrics];
    let current = passes.iter().all(|p| {
        recs.get(p.id())
            .is_some_and(|r| r.version == p.version() && r.data_version == stamp)
    });
    if !current {
        return Ok(None);
    }
    let tables: i64 = conn.query_row(
        "SELECT COUNT(*) FROM main.sqlite_master WHERE type = 'table' AND name IN ('link_positions', 'link_score')",
        [],
        |r| r.get(0),
    )?;
    if tables != 2 {
        return Ok(None);
    }
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'link_analysis_run'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    let Some(run) = raw.and_then(|r| serde_json::from_str::<StoredRun>(&r).ok()) else {
        return Ok(None);
    };
    Ok((run.rules == settings.position_rules && run.weighting == settings.weighting).then_some(run))
}

/// The pass ids to leave out of a crawl-end run for this file: the link
/// passes when its settings turn automatic analysis off (FR-012).
pub fn auto_skips(path: &std::path::Path) -> Result<Vec<&'static str>> {
    let conn = spiderlens_engine::crawl::store::connect_read_only(path)
        .map_err(|e| Error::invalid(e.to_string()))?;
    let settings = Features::detect(&conn)?.link_settings;
    Ok(if settings.auto_analyse {
        Vec::new()
    } else {
        LINK_PASSES.to_vec()
    })
}
