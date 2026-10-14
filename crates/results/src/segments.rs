//! Segment membership on the reader connection (spec 027 part B, plan D3).
//!
//! `temp.url_segment(url_id, seg)` holds the segment of every URL that has
//! one; `temp.segment_names(id, name, color, ord)` names them. Both live on
//! this reader's temporary storage, so the crawl file is never written. The
//! membership is decided in Rust by [`SegmentMatcher`], so user patterns
//! never become SQL. During a crawl the table is extended for new URLs and
//! for URLs fetched since the last update; changing the segments rebuilds it.

use std::collections::BTreeMap;

use rusqlite::{Connection, params};
use serde::Serialize;
use spiderlens_config::{Segment, SegmentMatcher, UrlFacts};
use ts_rs::TS;

use crate::reader::Reader;
use crate::{Result, sql};

/// Rows written per transaction while (re)building.
const BATCH: usize = 5_000;
/// How far back to re-check fetched URLs on each update.
const FETCH_ORDER_SLACK_MS: i64 = 5_000;

/// The column every tab gets: the URL's segment name, or null.
pub(crate) const SEGMENT_EXPR: &str = "(SELECT g.name FROM temp.url_segment s \
     JOIN temp.segment_names g ON g.id = s.seg WHERE s.url_id = u.id)";

/// URL counts per segment, for the segment filter (FR-B04).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SegmentCounts {
    /// Segment id to URL count, for every enabled segment (0 when empty).
    #[ts(type = "Record<number, number>")]
    pub counts: BTreeMap<u32, u64>,
    /// URLs in no segment.
    #[ts(type = "number")]
    pub no_segment: u64,
    /// The segments these counts are for, in the user's order.
    pub segments: Vec<Segment>,
}

pub(crate) struct State {
    segments: Vec<Segment>,
    matcher: SegmentMatcher,
    /// Highest url id classified so far.
    last_id: i64,
    /// Latest fetch time classified so far, for facts that arrive later.
    last_fetched: i64,
    /// Reader data version the table is current for.
    version: u64,
}

impl State {
    pub(crate) fn new(segments: Vec<Segment>) -> Self {
        Self {
            matcher: SegmentMatcher::new(&segments),
            segments,
            last_id: 0,
            last_fetched: 0,
            version: 0,
        }
    }
}

pub(crate) fn create_tables(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS url_segment (url_id INTEGER PRIMARY KEY, seg INTEGER NOT NULL);
         CREATE INDEX IF NOT EXISTS temp.url_segment_seg ON url_segment(seg);
         CREATE TEMP TABLE IF NOT EXISTS segment_names (
             id INTEGER PRIMARY KEY, name TEXT NOT NULL, color TEXT NOT NULL, ord INTEGER NOT NULL);",
    )
}

/// The segments stored with the crawl (its configuration), if any.
pub(crate) fn stored(conn: &Connection) -> Vec<Segment> {
    use rusqlite::OptionalExtension;
    conn.query_row("SELECT value FROM meta WHERE key = 'config'", [], |r| {
        r.get::<_, String>(0)
    })
    .optional()
    .ok()
    .flatten()
    .and_then(|c| serde_json::from_str::<spiderlens_config::StoredCrawlConfig>(&c).ok())
    .map(|s| s.config.segments)
    .unwrap_or_default()
}

fn write_names(conn: &Connection, segments: &[Segment]) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM temp.segment_names", [])?;
    let mut ins =
        conn.prepare("INSERT INTO temp.segment_names (id, name, color, ord) VALUES (?1, ?2, ?3, ?4)")?;
    for (ord, s) in segments.iter().enumerate().filter(|(_, s)| s.enabled) {
        let color = serde_json::to_value(s.color)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        ins.execute(params![s.id, s.name.trim(), color, ord as i64])?;
    }
    Ok(())
}

/// Classifies the URLs with `id > last_id` or fetched after
/// `last_fetched`, and returns the new watermarks.
fn classify(conn: &Connection, state: &State, extraction: bool) -> rusqlite::Result<(i64, i64)> {
    let page = if extraction {
        "(SELECT p.lang FROM pages p WHERE p.url_id = u.id), (SELECT p.word_count FROM pages p WHERE p.url_id = u.id)"
    } else {
        "NULL, NULL"
    };
    let facts = state.matcher.needs_facts();
    let sql = format!(
        "SELECT u.id, u.url, u.status, u.content_type, u.state, u.error_class, u.noindex, u.canonical,
                u.depth, COALESCE(u.fetched_at, 0), {page}
         FROM urls u WHERE u.id > ?1 {}",
        if facts { "OR u.fetched_at > ?2" } else { "" }
    );
    let mut stmt = conn.prepare(&sql)?;
    // Results are committed in batches a little after they are fetched, so
    // look back a few seconds for facts committed out of fetch order.
    let since = state.last_fetched - FETCH_ORDER_SLACK_MS;
    let mut rows = if facts {
        stmt.query(params![state.last_id, since])?
    } else {
        stmt.query(params![state.last_id])?
    };
    let (mut last_id, mut last_fetched) = (state.last_id, state.last_fetched);
    let mut set =
        conn.prepare_cached("INSERT OR REPLACE INTO temp.url_segment (url_id, seg) VALUES (?1, ?2)")?;
    let mut clear = conn.prepare_cached("DELETE FROM temp.url_segment WHERE url_id = ?1")?;
    let mut n = 0;
    conn.execute_batch("SAVEPOINT segments")?;
    while let Some(r) = rows.next()? {
        let id: i64 = r.get(0)?;
        let url: String = r.get(1)?;
        let status: Option<i64> = r.get(2)?;
        let content_type: Option<String> = r.get(3)?;
        let fetched: i64 = r.get(9)?;
        let lang: Option<String> = r.get(10)?;
        let indexable = facts.then(|| -> rusqlite::Result<bool> {
            Ok(sql::indexability(&sql::IndexabilityArgs {
                state: r.get(4)?,
                error_class: r.get(5)?,
                status,
                content_type: content_type.clone(),
                noindex: r.get::<_, Option<bool>>(6)?.unwrap_or(false),
                canonical: r.get(7)?,
                url: url.clone(),
            }) == "Indexable")
        });
        let indexable = indexable.transpose()?;
        let f = UrlFacts {
            url: &url,
            status,
            content_kind: Some(sql::content_kind(content_type.as_deref())),
            indexable,
            lang: lang.as_deref(),
            depth: r.get::<_, Option<i64>>(8)?.map(|d| d as u32),
            word_count: r.get(11)?,
        };
        match state.matcher.classify(&f) {
            Some(seg) => set.execute(params![id, seg])?,
            None => clear.execute([id])?,
        };
        last_id = last_id.max(id);
        last_fetched = last_fetched.max(fetched);
        n += 1;
        if n % BATCH == 0 {
            conn.execute_batch("RELEASE segments; SAVEPOINT segments")?;
        }
    }
    conn.execute_batch("RELEASE segments")?;
    Ok((last_id, last_fetched))
}

/// Fills membership on a fresh connection (exports) from `segments`, or
/// from the segments stored with the crawl. Returns whether any segment is
/// enabled.
pub(crate) fn prepare(
    conn: &Connection,
    segments: Option<&[Segment]>,
    features: &crate::Features,
) -> Result<bool> {
    let segments = segments.map_or_else(|| stored(conn), <[_]>::to_vec);
    let state = State::new(segments);
    write_names(conn, &state.segments)?;
    if state.matcher.is_empty() {
        return Ok(false);
    }
    classify(conn, &state, features.has(crate::Need::Extraction))?;
    Ok(true)
}

impl Reader {
    /// The segments this reader groups URLs by.
    pub fn segments(&self) -> &[Segment] {
        &self.segments.segments
    }

    /// Replaces the segments and rebuilds membership for the whole crawl
    /// (FR-B06). Views and counts pick the change up on their next refresh.
    pub fn set_segments(&mut self, segments: Vec<Segment>) -> Result<SegmentCounts> {
        self.segments = State::new(segments);
        self.conn.execute("DELETE FROM temp.url_segment", [])?;
        write_names(&self.conn, &self.segments.segments)?;
        self.bump_version();
        self.ensure_segments()?;
        self.segment_counts()
    }

    /// Brings membership up to date with the crawl's latest data.
    pub(crate) fn ensure_segments(&mut self) -> Result<()> {
        let v = self.version()?;
        if self.segments.version == v || self.segments.matcher.is_empty() {
            self.segments.version = v;
            return Ok(());
        }
        let extraction = self.features.has(crate::Need::Extraction);
        let (last_id, last_fetched) = classify(&self.conn, &self.segments, extraction)?;
        self.segments.last_id = last_id;
        self.segments.last_fetched = last_fetched;
        self.segments.version = v;
        Ok(())
    }

    /// URLs per segment over the whole crawl.
    pub fn segment_counts(&mut self) -> Result<SegmentCounts> {
        self.ensure_segments()?;
        let mut counts: BTreeMap<u32, u64> = self
            .segments
            .segments
            .iter()
            .filter(|s| s.enabled)
            .map(|s| (s.id, 0))
            .collect();
        let mut stmt = self
            .conn
            .prepare("SELECT seg, COUNT(*) FROM temp.url_segment GROUP BY seg")?;
        let mut in_segments = 0u64;
        for row in stmt.query_map([], |r| Ok((r.get::<_, u32>(0)?, r.get::<_, i64>(1)? as u64)))? {
            let (seg, n) = row?;
            counts.insert(seg, n);
            in_segments += n;
        }
        let total: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM urls", [], |r| r.get(0))?;
        Ok(SegmentCounts {
            counts,
            no_segment: (total as u64).saturating_sub(in_segments),
            segments: self.segments.segments.clone(),
        })
    }

    /// The id of the segment with this name (case-insensitive), for the CLI.
    pub fn segment_id(&self, name: &str) -> Option<u32> {
        self.segments
            .segments
            .iter()
            .find(|s| s.enabled && s.name.trim().eq_ignore_ascii_case(name.trim()))
            .map(|s| s.id)
    }
}
