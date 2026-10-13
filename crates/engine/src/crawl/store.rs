//! The crawl file: a SQLite database holding the frontier (research R5,
//! data-model.md). Spec 006 owns the file format and will add result tables.
//!
//! The unique index on `urls.url` is the seen-set; the partial index on
//! queued rows is the queue. Nothing here grows with the crawl in memory.

use std::cell::Cell;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use super::reason::{DiscoverySource, LinkKind, NotCrawledReason, UrlState};

pub const FORMAT: &str = "spiderlens-crawl";
pub const FORMAT_VERSION: i64 = 1;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("crawl file error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("{0}")]
    Format(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

const SCHEMA: &str = r#"
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE urls (
    id INTEGER PRIMARY KEY,
    url TEXT NOT NULL UNIQUE,
    host_key TEXT NOT NULL,
    internal INTEGER NOT NULL,
    depth INTEGER NOT NULL,
    state TEXT NOT NULL,
    check_only INTEGER NOT NULL DEFAULT 0,
    queue_seq INTEGER,
    reason TEXT,
    reason_detail TEXT,
    source TEXT NOT NULL,
    source_id INTEGER,
    in_sitemap INTEGER NOT NULL DEFAULT 0,
    via_nofollow INTEGER NOT NULL DEFAULT 0,
    robots_rule TEXT,
    status INTEGER,
    content_type TEXT,
    redirect_to INTEGER,
    title TEXT,
    error_class TEXT,
    error_message TEXT,
    fetched_at INTEGER
);
CREATE INDEX urls_queue ON urls(depth, queue_seq) WHERE state = 'queued';
CREATE INDEX urls_state ON urls(state, reason);
CREATE TABLE links (
    from_id INTEGER NOT NULL,
    to_id INTEGER NOT NULL,
    raw TEXT NOT NULL,
    kind TEXT NOT NULL,
    nofollow INTEGER NOT NULL,
    page_nofollow INTEGER NOT NULL,
    followed INTEGER NOT NULL
);
CREATE INDEX links_to ON links(to_id);
CREATE INDEX links_from ON links(from_id);
CREATE TABLE hosts (
    host_key TEXT PRIMARY KEY,
    robots_status TEXT NOT NULL,
    robots_http_status INTEGER,
    robots_error TEXT,
    crawl_delay_ms INTEGER,
    robots_txt TEXT
);
CREATE TABLE sitemaps (
    url TEXT PRIMARY KEY,
    parent TEXT,
    url_count INTEGER NOT NULL DEFAULT 0,
    error TEXT
);
"#;

/// The queue order key: depth first, then discovery order (FR-020).
pub type QueueKey = (u32, i64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueEntry {
    pub id: i64,
    pub url: String,
    pub host_key: String,
    pub depth: u32,
    pub seq: i64,
    pub check_only: bool,
    pub source: DiscoverySource,
}

impl QueueEntry {
    pub fn key(&self) -> QueueKey {
        (self.depth, self.seq)
    }
}

/// What the frontier should do with a newly discovered URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placement {
    Queue {
        check_only: bool,
        via_nofollow: bool,
    },
    NotCrawled {
        reason: NotCrawledReason,
        detail: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct Discovery<'a> {
    pub url: &'a str,
    pub host_key: &'a str,
    pub internal: bool,
    pub depth: u32,
    pub source: DiscoverySource,
    pub source_id: Option<i64>,
    pub in_sitemap: bool,
    pub placement: Placement,
}

/// The result of recording a discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recorded {
    /// A new URL; `queued` holds its queue entry when it was queued.
    New { id: i64, queued: Option<QueueEntry> },
    /// Seen before and its state did not change.
    Seen { id: i64 },
    /// Seen before and now (re)queued: a path-dependent reason was lifted or
    /// a shorter depth was found. `old_key` is its previous queue key.
    Requeued {
        id: i64,
        entry: QueueEntry,
        old_key: Option<QueueKey>,
    },
}

impl Recorded {
    pub fn id(&self) -> i64 {
        match self {
            Self::New { id, .. } | Self::Seen { id } | Self::Requeued { id, .. } => *id,
        }
    }
}

/// The response to one request, as stored on its URL row.
#[derive(Debug, Clone, Default)]
pub struct Response {
    pub status: Option<u16>,
    pub content_type: Option<String>,
    pub redirect_to: Option<i64>,
    pub title: Option<String>,
    pub error_class: Option<String>,
    pub error_message: Option<String>,
    /// Set when robots.txt is ignored but reported (FR-028).
    pub robots_rule: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HostRow {
    pub host_key: String,
    pub robots_status: String,
    pub robots_http_status: Option<u16>,
    pub robots_error: Option<String>,
    pub crawl_delay_ms: Option<i64>,
    pub robots_txt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitemapRow {
    pub url: String,
    pub parent: Option<String>,
    pub url_count: i64,
    pub error: Option<String>,
}

/// One row of `spiderlens report`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct UrlRow {
    pub url: String,
    pub state: UrlState,
    pub internal: bool,
    pub depth: u32,
    pub reason: Option<NotCrawledReason>,
    pub reason_detail: Option<String>,
    pub source: DiscoverySource,
    /// The page where the URL was first found.
    pub found_on: Option<String>,
    pub in_sitemap: bool,
    pub via_nofollow: bool,
    pub robots_rule: Option<String>,
    pub status: Option<u16>,
    pub error_class: Option<String>,
}

pub struct CrawlStore {
    conn: Connection,
    next_seq: i64,
    /// URLs per state, kept in step with every write below so live progress
    /// never has to count the table (spec 008 plan D3).
    live: Cell<StateCounts>,
}

/// URLs per state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StateCounts {
    pub queued: u64,
    pub in_flight: u64,
    pub crawled: u64,
    pub not_crawled: u64,
}

impl StateCounts {
    fn slot(&mut self, s: UrlState) -> &mut u64 {
        match s {
            UrlState::Queued => &mut self.queued,
            UrlState::InFlight => &mut self.in_flight,
            UrlState::Crawled => &mut self.crawled,
            UrlState::NotCrawled => &mut self.not_crawled,
        }
    }
}

/// One link from a crawled page, for [`CrawlStore::add_link`].
#[derive(Debug, Clone, Copy)]
pub struct NewLink<'a> {
    pub from: i64,
    pub to: i64,
    pub raw: &'a str,
    pub kind: LinkKind,
    /// The link itself carries rel=nofollow.
    pub nofollow: bool,
    /// The page's meta robots or X-Robots-Tag says nofollow.
    pub page_nofollow: bool,
    pub followed: bool,
}

/// Totals from [`CrawlStore::counts`].
#[derive(Debug, Clone, Default)]
pub struct Counts {
    pub by_state: Vec<(UrlState, u64)>,
    /// Not-crawled URLs by reason, in FR-038 order.
    pub by_reason: Vec<(NotCrawledReason, u64)>,
    /// External URLs that were checked (fetched without following links).
    pub external_checked: u64,
    /// External checks robots.txt did not allow.
    pub external_blocked: u64,
}

impl CrawlStore {
    /// Creates a new crawl file. An existing file is never overwritten.
    pub fn create(path: &Path) -> Result<Self> {
        if path.exists() {
            return Err(StoreError::Format(format!(
                "{} already exists; choose another file name",
                path.display()
            )));
        }
        let conn = Connection::open(path)?;
        Self::configure(&conn)?;
        conn.execute_batch(SCHEMA)?;
        let store = Self {
            conn,
            next_seq: 1,
            live: Cell::default(),
        };
        store.set_meta("format", FORMAT)?;
        store.set_meta("format_version", &FORMAT_VERSION.to_string())?;
        store.set_meta("status", "running")?;
        Ok(store)
    }

    /// Opens an existing crawl file and checks that it is one.
    pub fn open(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(StoreError::Format(format!("{} does not exist", path.display())));
        }
        let conn = Connection::open(path)?;
        Self::configure(&conn)?;
        let format: Option<String> = conn
            .query_row("SELECT value FROM meta WHERE key = 'format'", [], |r| r.get(0))
            .optional()
            .unwrap_or(None);
        if format.as_deref() != Some(FORMAT) {
            return Err(StoreError::Format(format!(
                "{} is not a SpiderLens crawl file",
                path.display()
            )));
        }
        let version: i64 = conn
            .query_row("SELECT value FROM meta WHERE key = 'format_version'", [], |r| {
                r.get::<_, String>(0)
            })?
            .parse()
            .unwrap_or(0);
        if version > FORMAT_VERSION {
            return Err(StoreError::Format(format!(
                "{} was written by a newer SpiderLens (crawl format {version}); update SpiderLens to open it",
                path.display()
            )));
        }
        let max_seq: i64 =
            conn.query_row("SELECT COALESCE(MAX(queue_seq), 0) FROM urls", [], |r| r.get(0))?;
        let store = Self {
            conn,
            next_seq: max_seq + 1,
            live: Cell::default(),
        };
        let mut live = StateCounts::default();
        for (state, n) in store.state_counts()? {
            *live.slot(state) = n;
        }
        store.live.set(live);
        Ok(store)
    }

    /// URLs per state right now, without touching the file.
    pub fn live_counts(&self) -> StateCounts {
        self.live.get()
    }

    /// `n` rows moved from `from` (None for new rows) to `to`.
    fn shift(&self, from: Option<UrlState>, to: UrlState, n: usize) {
        let mut live = self.live.get();
        if let Some(f) = from {
            let slot = live.slot(f);
            *slot = slot.saturating_sub(n as u64);
        }
        *live.slot(to) += n as u64;
        self.live.set(live);
    }

    fn configure(conn: &Connection) -> Result<()> {
        // WAL keeps readers working during writes; NORMAL sync is safe with
        // WAL (a crash loses at most the last transactions, never consistency).
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "OFF")?;
        Ok(())
    }

    /// Starts a batch. One batch per processed page makes its discoveries
    /// all-or-nothing (spec 006 FR-004).
    pub fn begin(&self) -> Result<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        Ok(())
    }

    pub fn commit(&self) -> Result<()> {
        self.conn.execute_batch("COMMIT")?;
        Ok(())
    }

    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .prepare_cached("SELECT value FROM meta WHERE key = ?1")?
            .query_row([key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn
            .prepare_cached(
                "INSERT INTO meta(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
            )?
            .execute([key, value])?;
        Ok(())
    }

    fn take_seq(&mut self) -> i64 {
        let s = self.next_seq;
        self.next_seq += 1;
        s
    }

    /// Records a discovered URL: inserts it if new, otherwise applies the
    /// upgrade rules (FR-039, research R3) and the sitemap flag.
    pub fn record(&mut self, d: &Discovery<'_>) -> Result<Recorded> {
        let existing = self
            .conn
            .prepare_cached(
                "SELECT id, state, reason, depth, queue_seq, check_only FROM urls WHERE url = ?1",
            )?
            .query_row([d.url], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, u32>(3)?,
                    r.get::<_, Option<i64>>(4)?,
                    r.get::<_, bool>(5)?,
                ))
            })
            .optional()?;

        let Some((id, state, reason, depth, seq, check_only)) = existing else {
            return self.insert(d);
        };
        if d.in_sitemap {
            self.conn
                .prepare_cached("UPDATE urls SET in_sitemap = 1 WHERE id = ?1")?
                .execute([id])?;
        }
        let Placement::Queue {
            check_only: false,
            via_nofollow,
        } = d.placement
        else {
            return Ok(Recorded::Seen { id });
        };
        let state = UrlState::parse(&state);
        let upgradable = state == Some(UrlState::NotCrawled)
            && reason
                .as_deref()
                .and_then(NotCrawledReason::parse)
                .is_some_and(NotCrawledReason::can_upgrade);
        let shorter = state == Some(UrlState::Queued) && !check_only && d.depth < depth;
        if !upgradable && !shorter {
            return Ok(Recorded::Seen { id });
        }
        let new_seq = self.take_seq();
        self.conn
            .prepare_cached(
                "UPDATE urls SET state = 'queued', reason = NULL, reason_detail = NULL, depth = ?2,
                 queue_seq = ?3, via_nofollow = ?4 WHERE id = ?1",
            )?
            .execute(params![id, d.depth, new_seq, via_nofollow])?;
        if upgradable {
            self.shift(Some(UrlState::NotCrawled), UrlState::Queued, 1);
        }
        let source = self.source_of(id)?;
        Ok(Recorded::Requeued {
            id,
            entry: QueueEntry {
                id,
                url: d.url.to_string(),
                host_key: d.host_key.to_string(),
                depth: d.depth,
                seq: new_seq,
                check_only: false,
                source,
            },
            old_key: shorter.then(|| (depth, seq.unwrap_or(0))),
        })
    }

    fn source_of(&self, id: i64) -> Result<DiscoverySource> {
        let s: String = self
            .conn
            .prepare_cached("SELECT source FROM urls WHERE id = ?1")?
            .query_row([id], |r| r.get(0))?;
        Ok(DiscoverySource::parse(&s).unwrap_or(DiscoverySource::Link))
    }

    fn insert(&mut self, d: &Discovery<'_>) -> Result<Recorded> {
        let (state, check_only, via_nofollow, reason, detail, seq) = match &d.placement {
            Placement::Queue {
                check_only,
                via_nofollow,
            } => {
                let seq = self.take_seq();
                let reason = check_only.then_some(NotCrawledReason::External);
                (
                    UrlState::Queued,
                    *check_only,
                    *via_nofollow,
                    reason,
                    None,
                    Some(seq),
                )
            }
            Placement::NotCrawled { reason, detail } => (
                UrlState::NotCrawled,
                false,
                false,
                Some(*reason),
                detail.clone(),
                None,
            ),
        };
        self.conn
            .prepare_cached(
                "INSERT INTO urls (url, host_key, internal, depth, state, check_only, queue_seq, reason,
                 reason_detail, source, source_id, in_sitemap, via_nofollow)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            )?
            .execute(params![
                d.url,
                d.host_key,
                d.internal,
                d.depth,
                state.as_str(),
                check_only,
                seq,
                reason.map(NotCrawledReason::as_str),
                detail,
                d.source.as_str(),
                d.source_id,
                d.in_sitemap,
                via_nofollow,
            ])?;
        let id = self.conn.last_insert_rowid();
        self.shift(None, state, 1);
        let queued = seq.map(|seq| QueueEntry {
            id,
            url: d.url.to_string(),
            host_key: d.host_key.to_string(),
            depth: d.depth,
            seq,
            check_only,
            source: d.source,
        });
        Ok(Recorded::New { id, queued })
    }

    pub fn add_link(&self, link: &NewLink) -> Result<()> {
        let NewLink {
            from,
            to,
            raw,
            kind,
            nofollow,
            page_nofollow,
            followed,
        } = *link;
        self.conn
            .prepare_cached(
                "INSERT INTO links (from_id, to_id, raw, kind, nofollow, page_nofollow, followed)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?
            .execute(params![
                from,
                to,
                raw,
                kind.as_str(),
                nofollow,
                page_nofollow,
                followed
            ])?;
        Ok(())
    }

    /// Up to `limit` queued rows after `after`, in queue order.
    pub fn queued_after(&self, after: Option<QueueKey>, limit: usize) -> Result<Vec<QueueEntry>> {
        let (d, s) = after.unwrap_or((0, i64::MIN));
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, url, host_key, depth, queue_seq, check_only, source FROM urls
             WHERE state = 'queued' AND (depth > ?1 OR (depth = ?1 AND queue_seq > ?2))
             ORDER BY depth, queue_seq LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![d, s, limit as i64], |r| {
            Ok(QueueEntry {
                id: r.get(0)?,
                url: r.get(1)?,
                host_key: r.get(2)?,
                depth: r.get(3)?,
                seq: r.get(4)?,
                check_only: r.get(5)?,
                source: DiscoverySource::parse(&r.get::<_, String>(6)?).unwrap_or(DiscoverySource::Link),
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn mark_in_flight(&self, id: i64) -> Result<()> {
        let n = self
            .conn
            .prepare_cached("UPDATE urls SET state = 'in_flight' WHERE id = ?1 AND state = 'queued'")?
            .execute([id])?;
        self.shift(Some(UrlState::Queued), UrlState::InFlight, n);
        Ok(())
    }

    /// Stores a response. Page requests end `crawled`; external status
    /// checks stay `not_crawled` with reason "external" (data-model.md).
    pub fn record_response(&self, id: i64, check_only: bool, r: &Response, now_ms: i64) -> Result<()> {
        let state = if check_only {
            UrlState::NotCrawled
        } else {
            UrlState::Crawled
        };
        let n = self
            .conn
            .prepare_cached(
                "UPDATE urls SET state = ?2, status = ?3, content_type = ?4, redirect_to = ?5, title = ?6,
                 error_class = ?7, error_message = ?8, robots_rule = COALESCE(?9, robots_rule), fetched_at = ?10
                 WHERE id = ?1 AND state = 'in_flight'",
            )?
            .execute(params![
                id,
                state.as_str(),
                r.status,
                r.content_type,
                r.redirect_to,
                r.title,
                r.error_class,
                r.error_message,
                r.robots_rule,
                now_ms,
            ])?;
        self.shift(Some(UrlState::InFlight), state, n);
        Ok(())
    }

    /// Ends a queued URL without a request.
    pub fn set_not_crawled(&self, id: i64, reason: NotCrawledReason, detail: Option<&str>) -> Result<()> {
        let n = self
            .conn
            .prepare_cached(
                "UPDATE urls SET state = 'not_crawled', reason = ?2, reason_detail = ?3
                 WHERE id = ?1 AND state = 'queued'",
            )?
            .execute(params![id, reason.as_str(), detail])?;
        self.shift(Some(UrlState::Queued), UrlState::NotCrawled, n);
        Ok(())
    }

    /// Ends a queued external check that robots.txt does not allow. The
    /// reason stays "external"; `blocked` says why it was not checked and
    /// `robots_rule` holds the rule, so the URL does not look unchecked.
    pub fn drop_check(&self, id: i64, blocked: NotCrawledReason, rule: Option<&str>) -> Result<()> {
        let n = self
            .conn
            .prepare_cached(
                "UPDATE urls SET state = 'not_crawled', reason_detail = ?2, robots_rule = ?3
                 WHERE id = ?1 AND state = 'queued'",
            )?
            .execute(params![id, blocked.label(), rule])?;
        self.shift(Some(UrlState::Queued), UrlState::NotCrawled, n);
        Ok(())
    }

    /// Every queued page row (not external checks), in id order, for the
    /// URL limit and stop passes.
    pub fn queued_pages_after(&self, after_id: i64, limit: usize) -> Result<Vec<QueueEntry>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, url, host_key, depth, queue_seq, check_only, source FROM urls
             WHERE state = 'queued' AND check_only = 0 AND id > ?1 ORDER BY id LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![after_id, limit as i64], |r| {
            Ok(QueueEntry {
                id: r.get(0)?,
                url: r.get(1)?,
                host_key: r.get(2)?,
                depth: r.get(3)?,
                seq: r.get(4)?,
                check_only: r.get(5)?,
                source: DiscoverySource::parse(&r.get::<_, String>(6)?).unwrap_or(DiscoverySource::Link),
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Stop: every queued page becomes "Crawl stopped"; queued external checks
    /// end unchecked (FR-022, US4-4).
    pub fn stop_queue(&self) -> Result<()> {
        let a = self.conn.execute(
            "UPDATE urls SET state = 'not_crawled', reason = 'crawl_stopped'
             WHERE state = 'queued' AND check_only = 0",
            [],
        )?;
        let b = self.conn.execute(
            "UPDATE urls SET state = 'not_crawled' WHERE state = 'queued' AND check_only = 1",
            [],
        )?;
        self.shift(Some(UrlState::Queued), UrlState::NotCrawled, a + b);
        Ok(())
    }

    /// Resume: rows left in flight by a crash or a pause, and stopped rows,
    /// go back to the queue. Returns how many were requeued.
    pub fn requeue_for_resume(&self) -> Result<usize> {
        let a = self
            .conn
            .execute("UPDATE urls SET state = 'queued' WHERE state = 'in_flight'", [])?;
        let b = self.conn.execute(
            "UPDATE urls SET state = 'queued', reason = NULL, reason_detail = NULL
             WHERE state = 'not_crawled' AND reason = 'crawl_stopped'",
            [],
        )?;
        self.shift(Some(UrlState::InFlight), UrlState::Queued, a);
        self.shift(Some(UrlState::NotCrawled), UrlState::Queued, b);
        Ok(a + b)
    }

    /// Requests started for crawling, which count toward the URL limit.
    pub fn pages_started(&self) -> Result<u64> {
        Ok(self.conn.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM urls WHERE state IN ('crawled', 'in_flight') AND check_only = 0",
            [],
            |r| r.get(0),
        )? as u64)
    }

    pub fn has_queued(&self) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM urls WHERE state = 'queued')",
            [],
            |r| r.get(0),
        )?)
    }

    /// The List mode URLs, as recorded at the start of the crawl.
    pub fn list_urls(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT url FROM urls WHERE source = 'list' ORDER BY id")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn url_of(&self, id: i64) -> Result<String> {
        Ok(self
            .conn
            .prepare_cached("SELECT url FROM urls WHERE id = ?1")?
            .query_row([id], |r| r.get(0))?)
    }

    pub fn upsert_host(&self, h: &HostRow) -> Result<()> {
        self.conn
            .prepare_cached(
                "INSERT INTO hosts (host_key, robots_status, robots_http_status, robots_error, crawl_delay_ms, robots_txt)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(host_key) DO UPDATE SET robots_status = ?2, robots_http_status = ?3,
                 robots_error = ?4, crawl_delay_ms = ?5, robots_txt = ?6",
            )?
            .execute(params![
                h.host_key,
                h.robots_status,
                h.robots_http_status,
                h.robots_error,
                h.crawl_delay_ms,
                h.robots_txt
            ])?;
        Ok(())
    }

    pub fn hosts(&self) -> Result<Vec<HostRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT host_key, robots_status, robots_http_status, robots_error, crawl_delay_ms, robots_txt
             FROM hosts ORDER BY host_key",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(HostRow {
                host_key: r.get(0)?,
                robots_status: r.get(1)?,
                robots_http_status: r.get(2)?,
                robots_error: r.get(3)?,
                crawl_delay_ms: r.get(4)?,
                robots_txt: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn upsert_sitemap(&self, s: &SitemapRow) -> Result<()> {
        self.conn
            .prepare_cached(
                "INSERT INTO sitemaps (url, parent, url_count, error) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(url) DO UPDATE SET url_count = ?3, error = ?4",
            )?
            .execute(params![s.url, s.parent, s.url_count, s.error])?;
        Ok(())
    }

    pub fn sitemaps(&self) -> Result<Vec<SitemapRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT url, parent, url_count, error FROM sitemaps ORDER BY rowid")?;
        let rows = stmt.query_map([], |r| {
            Ok(SitemapRow {
                url: r.get(0)?,
                parent: r.get(1)?,
                url_count: r.get(2)?,
                error: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Counts by state, and not-crawled counts by reason.
    /// URLs per state, for live progress.
    pub fn state_counts(&self) -> Result<Vec<(UrlState, u64)>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT state, COUNT(*) FROM urls GROUP BY state")?;
        let mut out = Vec::new();
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
            let (s, n) = row?;
            if let Some(s) = UrlState::parse(&s) {
                out.push((s, n as u64));
            }
        }
        Ok(out)
    }

    /// Crawled pages that failed or answered 4xx/5xx (read once on resume).
    pub fn page_errors(&self) -> Result<u64> {
        Ok(self.conn.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM urls WHERE state = 'crawled' AND (error_class IS NOT NULL OR status >= 400)",
            [],
            |r| r.get(0),
        )? as u64)
    }

    pub fn counts(&self) -> Result<Counts> {
        let by_state = self.state_counts()?;
        let mut by_reason = Vec::new();
        let mut stmt = self.conn.prepare(
            "SELECT reason, COUNT(*) FROM urls WHERE state = 'not_crawled' GROUP BY reason ORDER BY reason",
        )?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?)))? {
            let (reason, n) = row?;
            let n = n as u64;
            if let Some(reason) = reason.as_deref().and_then(NotCrawledReason::parse) {
                by_reason.push((reason, n));
            }
        }
        by_reason.sort();
        let checked = self.conn.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM urls WHERE check_only = 1 AND fetched_at IS NOT NULL",
            [],
            |r| r.get(0),
        )? as u64;
        let blocked = self.conn.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM urls
             WHERE check_only = 1 AND state = 'not_crawled' AND fetched_at IS NULL AND reason_detail IS NOT NULL",
            [],
            |r| r.get(0),
        )? as u64;
        Ok(Counts {
            by_state,
            by_reason,
            external_checked: checked,
            external_blocked: blocked,
        })
    }

    /// Rows for `spiderlens report`, filtered by state and reasons.
    pub fn rows(
        &self,
        state: Option<UrlState>,
        reasons: &[NotCrawledReason],
        mut each: impl FnMut(UrlRow) -> Result<()>,
    ) -> Result<()> {
        let mut sql = String::from(
            "SELECT u.url, u.state, u.internal, u.depth, u.reason, u.reason_detail, u.source, s.url,
             u.in_sitemap, u.via_nofollow, u.robots_rule, u.status, u.error_class
             FROM urls u LEFT JOIN urls s ON s.id = u.source_id WHERE 1 = 1",
        );
        if let Some(st) = state {
            sql.push_str(&format!(" AND u.state = '{}'", st.as_str()));
        }
        if !reasons.is_empty() {
            let list: Vec<String> = reasons.iter().map(|r| format!("'{}'", r.as_str())).collect();
            sql.push_str(&format!(" AND u.reason IN ({})", list.join(",")));
        }
        sql.push_str(" ORDER BY u.id");
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let state: String = r.get(1)?;
            let reason: Option<String> = r.get(4)?;
            let source: String = r.get(6)?;
            each(UrlRow {
                url: r.get(0)?,
                state: UrlState::parse(&state).unwrap_or(UrlState::Queued),
                internal: r.get(2)?,
                depth: r.get(3)?,
                reason: reason.as_deref().and_then(NotCrawledReason::parse),
                reason_detail: r.get(5)?,
                source: DiscoverySource::parse(&source).unwrap_or(DiscoverySource::Link),
                found_on: r.get(7)?,
                in_sitemap: r.get(8)?,
                via_nofollow: r.get(9)?,
                robots_rule: r.get(10)?,
                status: r.get(11)?,
                error_class: r.get(12)?,
            })?;
        }
        Ok(())
    }

    /// Links whose target is `url`, for tests and the detail view.
    pub fn links_to(&self, url: &str) -> Result<Vec<(String, String, bool, bool)>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.url, l.raw, l.nofollow, l.followed FROM links l
             JOIN urls f ON f.id = l.from_id JOIN urls t ON t.id = l.to_id WHERE t.url = ?1 ORDER BY l.rowid",
        )?;
        let rows = stmt.query_map([url], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, CrawlStore) {
        let dir = tempfile::tempdir().unwrap();
        let s = CrawlStore::create(&dir.path().join("t.spiderlens")).unwrap();
        (dir, s)
    }

    fn disc(url: &str, depth: u32, placement: Placement) -> Discovery<'_> {
        Discovery {
            url,
            host_key: "https://a.test:443",
            internal: true,
            depth,
            source: DiscoverySource::Link,
            source_id: None,
            in_sitemap: false,
            placement,
        }
    }

    const QUEUE: Placement = Placement::Queue {
        check_only: false,
        via_nofollow: false,
    };

    #[test]
    fn each_url_is_recorded_once() {
        let (_d, mut s) = store();
        let a = s.record(&disc("https://a.test/a", 1, QUEUE)).unwrap();
        assert!(matches!(a, Recorded::New { queued: Some(_), .. }));
        let again = s.record(&disc("https://a.test/a", 1, QUEUE)).unwrap();
        assert_eq!(again, Recorded::Seen { id: a.id() });
        assert_eq!(s.queued_after(None, 10).unwrap().len(), 1);
    }

    #[test]
    fn path_dependent_reasons_are_upgraded() {
        let (_d, mut s) = store();
        let deep = Placement::NotCrawled {
            reason: NotCrawledReason::OverDepthLimit,
            detail: Some("2".into()),
        };
        let r = s.record(&disc("https://a.test/x", 3, deep)).unwrap();
        let up = s.record(&disc("https://a.test/x", 2, QUEUE)).unwrap();
        assert!(matches!(up, Recorded::Requeued { id, old_key: None, .. } if id == r.id()));
        // URL-based reasons never change.
        let excluded = Placement::NotCrawled {
            reason: NotCrawledReason::Excluded,
            detail: None,
        };
        s.record(&disc("https://a.test/y", 1, excluded)).unwrap();
        assert!(matches!(
            s.record(&disc("https://a.test/y", 1, QUEUE)).unwrap(),
            Recorded::Seen { .. }
        ));
    }

    #[test]
    fn shorter_depth_moves_a_queued_url() {
        let (_d, mut s) = store();
        s.record(&disc("https://a.test/x", 3, QUEUE)).unwrap();
        let moved = s.record(&disc("https://a.test/x", 1, QUEUE)).unwrap();
        assert!(
            matches!(moved, Recorded::Requeued { old_key: Some((3, _)), ref entry, .. } if entry.depth == 1)
        );
    }

    #[test]
    fn queue_is_ordered_by_depth_then_discovery() {
        let (_d, mut s) = store();
        for (u, d) in [
            ("https://a.test/2a", 2),
            ("https://a.test/1a", 1),
            ("https://a.test/1b", 1),
        ] {
            s.record(&disc(u, d, QUEUE)).unwrap();
        }
        let urls: Vec<_> = s
            .queued_after(None, 10)
            .unwrap()
            .into_iter()
            .map(|e| e.url)
            .collect();
        assert_eq!(
            urls,
            ["https://a.test/1a", "https://a.test/1b", "https://a.test/2a"]
        );
        let first = s.queued_after(None, 1).unwrap()[0].key();
        assert_eq!(s.queued_after(Some(first), 10).unwrap().len(), 2);
    }

    #[test]
    fn stop_and_resume() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.spiderlens");
        {
            let mut s = CrawlStore::create(&path).unwrap();
            let a = s.record(&disc("https://a.test/a", 0, QUEUE)).unwrap().id();
            s.record(&disc("https://a.test/b", 1, QUEUE)).unwrap();
            s.mark_in_flight(a).unwrap();
            s.stop_queue().unwrap();
            let reasons = s.counts().unwrap().by_reason;
            assert_eq!(reasons, vec![(NotCrawledReason::CrawlStopped, 1)]);
        }
        assert!(CrawlStore::create(&path).is_err(), "never overwrites");
        let s = CrawlStore::open(&path).unwrap();
        assert_eq!(s.requeue_for_resume().unwrap(), 2);
        assert_eq!(s.queued_after(None, 10).unwrap().len(), 2);
    }

    #[test]
    fn foreign_files_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.db");
        Connection::open(&p)
            .unwrap()
            .execute_batch("CREATE TABLE t(x)")
            .unwrap();
        assert!(matches!(CrawlStore::open(&p), Err(StoreError::Format(_))));
    }

    #[test]
    fn two_hundred_thousand_urls_stay_on_disk() {
        let (_d, mut s) = store();
        s.begin().unwrap();
        for i in 0..200_000 {
            let u = format!("https://a.test/p/{i}");
            s.record(&disc(&u, 1 + (i % 7), QUEUE)).unwrap();
        }
        s.commit().unwrap();
        // Paging through the queue never needs more than one page in memory.
        let mut after = None;
        let mut total = 0;
        loop {
            let page = s.queued_after(after, 4096).unwrap();
            if page.is_empty() {
                break;
            }
            assert!(page.windows(2).all(|w| w[0].key() < w[1].key()));
            after = page.last().map(QueueEntry::key);
            total += page.len();
        }
        assert_eq!(total, 200_000);
    }
}
