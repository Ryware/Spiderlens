//! Integration runs and results in the crawl file (plan D7, D8,
//! data-model.md). Every write here happens under 006's writer lock, held
//! by the [`spiderlens_engine::crawl::Update`] the caller opened.

use jiff::Timestamp;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Provider, Result};

/// The meta key listing which integrations have data in this crawl file
/// (comma-separated provider feature ids). Kept apart from the crawl's own
/// `data_features`, which the engine owns.
pub const FEATURES_KEY: &str = "integration_features";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RunState {
    Queued,
    Running,
    Finished,
    PausedUser,
    PausedQuota,
    Cancelled,
    Failed,
}

impl RunState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Finished => "finished",
            Self::PausedUser => "paused_user",
            Self::PausedQuota => "paused_quota",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "queued" => Self::Queued,
            "running" => Self::Running,
            "finished" => Self::Finished,
            "paused_user" => Self::PausedUser,
            "paused_quota" => Self::PausedQuota,
            "cancelled" => Self::Cancelled,
            _ => Self::Failed,
        }
    }

    pub fn is_resumable(self) -> bool {
        matches!(self, Self::PausedUser | Self::PausedQuota | Self::Running | Self::Queued)
    }
}

/// One run as the UI and CLI show it (data-model.md `RunProgress`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunProgress {
    #[ts(type = "number")]
    pub id: i64,
    pub provider: Provider,
    pub account_label: Option<String>,
    pub property: Option<String>,
    pub params: serde_json::Value,
    pub state: RunState,
    #[ts(type = "number")]
    pub requested: i64,
    #[ts(type = "number")]
    pub done: i64,
    #[ts(type = "number")]
    pub failed: i64,
    #[ts(type = "number | null")]
    pub matched: Option<i64>,
    #[ts(type = "number | null")]
    pub unmatched: Option<i64>,
    #[ts(type = "number | null")]
    pub usage: Option<i64>,
    /// RFC 3339.
    pub resume_at: Option<String>,
    pub message: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

fn ts(sec: Option<i64>) -> Option<String> {
    sec.and_then(|s| Timestamp::from_second(s).ok()).map(|t| t.to_string())
}

pub fn create_run(
    conn: &Connection,
    provider: Provider,
    account_label: Option<&str>,
    property: Option<&str>,
    params: &serde_json::Value,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO integration_runs (provider, account_label, property, params, state, created_at, started_at, app_version)
         VALUES (?1, ?2, ?3, ?4, 'running', ?5, ?5, ?6)",
        params![
            provider.id(),
            account_label,
            property,
            params.to_string(),
            Timestamp::now().as_second(),
            spiderlens_engine::BuildInfo::current().product_version
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Counts {
    pub requested: i64,
    pub done: i64,
    pub failed: i64,
    pub matched: Option<i64>,
    pub unmatched: Option<i64>,
    pub usage: Option<i64>,
}

pub fn save_progress(conn: &Connection, id: i64, c: &Counts, cursor: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE integration_runs SET requested = ?2, done = ?3, failed = ?4, matched = ?5,
           unmatched = ?6, usage = ?7, cursor = COALESCE(?8, cursor) WHERE id = ?1",
        params![id, c.requested, c.done, c.failed, c.matched, c.unmatched, c.usage, cursor],
    )?;
    Ok(())
}

pub fn set_state(
    conn: &Connection,
    id: i64,
    state: RunState,
    message: Option<&str>,
    resume_at: Option<Timestamp>,
) -> Result<()> {
    let finished = matches!(state, RunState::Finished | RunState::Cancelled | RunState::Failed)
        .then(|| Timestamp::now().as_second());
    conn.execute(
        "UPDATE integration_runs SET state = ?2, message = ?3, resume_at = ?4,
           finished_at = COALESCE(?5, finished_at) WHERE id = ?1",
        params![id, state.as_str(), message, resume_at.map(|t| t.as_second()), finished],
    )?;
    if finished.is_some() {
        let (provider, done, failed): (String, i64, i64) = conn.query_row(
            "SELECT provider, done, failed FROM integration_runs WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        conn.execute(
            "INSERT INTO events (at, kind, app_version, os, detail) VALUES (?1, 'integration_run', ?2, ?3, ?4)",
            params![
                Timestamp::now().as_millisecond(),
                spiderlens_engine::BuildInfo::current().product_version,
                spiderlens_engine::crawl::store::os_name(),
                serde_json::json!({"provider": provider, "run": id, "state": state.as_str(), "done": done, "failed": failed})
                    .to_string()
            ],
        )?;
    }
    if state == RunState::Finished {
        let provider: String =
            conn.query_row("SELECT provider FROM integration_runs WHERE id = ?1", [id], |r| r.get(0))?;
        if let Some(p) = Provider::from_id(&provider) {
            add_feature(conn, p.feature_id())?;
        }
    }
    Ok(())
}

pub fn features(conn: &Connection) -> Result<Vec<String>> {
    let v: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = ?1", [FEATURES_KEY], |r| r.get(0))
        .optional()?;
    Ok(v.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect())
}

fn write_features(conn: &Connection, list: &[String]) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
        params![FEATURES_KEY, list.join(",")],
    )?;
    Ok(())
}

fn add_feature(conn: &Connection, id: &str) -> Result<()> {
    let mut list = features(conn)?;
    if !list.iter().any(|f| f == id) {
        list.push(id.to_string());
        write_features(conn, &list)?;
    }
    Ok(())
}

fn row_to_progress(r: &rusqlite::Row<'_>) -> rusqlite::Result<RunProgress> {
    let provider: String = r.get("provider")?;
    let params: String = r.get("params")?;
    Ok(RunProgress {
        id: r.get("id")?,
        provider: Provider::from_id(&provider).unwrap_or(Provider::SearchConsole),
        account_label: r.get("account_label")?,
        property: r.get("property")?,
        params: serde_json::from_str(&params).unwrap_or_default(),
        state: RunState::parse(&r.get::<_, String>("state")?),
        requested: r.get("requested")?,
        done: r.get("done")?,
        failed: r.get("failed")?,
        matched: r.get("matched")?,
        unmatched: r.get("unmatched")?,
        usage: r.get("usage")?,
        resume_at: ts(r.get("resume_at")?),
        message: r.get("message")?,
        created_at: ts(Some(r.get("created_at")?)).unwrap_or_default(),
        finished_at: ts(r.get("finished_at")?),
    })
}

/// Every run of the crawl, newest first. Older crawl files have none.
pub fn runs(conn: &Connection) -> Result<Vec<RunProgress>> {
    if !has_tables(conn)? {
        return Ok(Vec::new());
    }
    let mut s = conn.prepare("SELECT * FROM integration_runs ORDER BY id DESC")?;
    let v = s.query_map([], row_to_progress)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn run(conn: &Connection, id: i64) -> Result<Option<RunProgress>> {
    Ok(conn
        .query_row("SELECT * FROM integration_runs WHERE id = ?1", [id], row_to_progress)
        .optional()?)
}

pub fn cursor(conn: &Connection, id: i64) -> Result<Option<String>> {
    Ok(conn.query_row("SELECT cursor FROM integration_runs WHERE id = ?1", [id], |r| r.get(0))?)
}

/// Whether the file has the integration tables (format with spec 024's step).
pub fn has_tables(conn: &Connection) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'integration_runs'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// The tables holding each provider's per-URL data.
fn tables_of(p: Provider) -> &'static [&'static str] {
    match p {
        Provider::SearchConsole => &["gsc_pages", "gsc_queries"],
        Provider::UrlInspection => &["gsc_inspection"],
        Provider::Analytics => &["ga4_pages"],
        Provider::FieldData => &["cwv_field"],
        Provider::LabTests => &["psi_lab"],
        Provider::Ahrefs | Provider::Majestic | Provider::Moz => &["link_metrics"],
        Provider::Sheets => &[],
    }
}

/// Removes one provider's data, or all integration data, in one
/// transaction (FR-011). The crawl's own data is never touched.
pub fn remove(conn: &Connection, provider: Option<Provider>) -> Result<()> {
    if !has_tables(conn)? {
        return Ok(());
    }
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let r = (|| -> Result<()> {
        let providers: Vec<Provider> = match provider {
            Some(p) => vec![p],
            None => Provider::ALL.to_vec(),
        };
        for p in &providers {
            for t in tables_of(*p) {
                if *t == "link_metrics" {
                    conn.execute("DELETE FROM link_metrics WHERE provider = ?1", [p.id()])?;
                } else {
                    conn.execute(&format!("DELETE FROM {t}"), [])?;
                }
            }
            conn.execute("DELETE FROM integration_status WHERE provider = ?1", [p.id()])?;
            conn.execute(
                "DELETE FROM integration_queue WHERE run_id IN (SELECT id FROM integration_runs WHERE provider = ?1)",
                [p.id()],
            )?;
            let source = match p {
                Provider::SearchConsole => Some("search_console"),
                Provider::Analytics => Some("analytics"),
                _ => None,
            };
            if let Some(s) = source {
                conn.execute("DELETE FROM orphan_candidates WHERE source = ?1", [s])?;
            }
            conn.execute("DELETE FROM integration_runs WHERE provider = ?1", [p.id()])?;
        }
        let still: Vec<String> = features(conn)?
            .into_iter()
            .filter(|f| {
                let gone = providers.iter().any(|p| p.feature_id() == f);
                let link_left = f == "link_metrics"
                    && conn
                        .query_row("SELECT 1 FROM link_metrics LIMIT 1", [], |_| Ok(()))
                        .optional()
                        .ok()
                        .flatten()
                        .is_some();
                !gone || link_left
            })
            .collect();
        write_features(conn, &still)?;
        Ok(())
    })();
    match r {
        Ok(()) => conn.execute_batch("COMMIT")?,
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(e);
        }
    }
    Ok(())
}

/// One page of orphan candidates (FR-023).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OrphanCandidate {
    pub url: String,
    pub source: String,
    #[ts(type = "number | null")]
    pub clicks: Option<i64>,
    #[ts(type = "number | null")]
    pub impressions: Option<i64>,
    #[ts(type = "number | null")]
    pub sessions: Option<i64>,
}

pub fn orphans(conn: &Connection, source: Option<&str>, offset: i64, limit: i64) -> Result<(Vec<OrphanCandidate>, i64)> {
    if !has_tables(conn)? {
        return Ok((Vec::new(), 0));
    }
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM orphan_candidates WHERE ?1 IS NULL OR source = ?1",
        [source],
        |r| r.get(0),
    )?;
    let mut s = conn.prepare(
        "SELECT url, source, clicks, impressions, sessions FROM orphan_candidates
         WHERE ?1 IS NULL OR source = ?1
         ORDER BY COALESCE(clicks, 0) + COALESCE(sessions, 0) DESC, COALESCE(impressions, 0) DESC, url
         LIMIT ?2 OFFSET ?3",
    )?;
    let rows = s
        .query_map(params![source, limit, offset], |r| {
            Ok(OrphanCandidate {
                url: r.get(0)?,
                source: r.get(1)?,
                clicks: r.get(2)?,
                impressions: r.get(3)?,
                sessions: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok((rows, total))
}
