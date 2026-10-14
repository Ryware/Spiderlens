//! The crawl file: a SQLite database holding the frontier (research R5,
//! data-model.md). Spec 006 owns the file format and will add result tables.
//!
//! The unique index on `urls.url` is the seen-set; the partial index on
//! queued rows is the queue. Nothing here grows with the crawl in memory.

use std::cell::Cell;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{Connection, OptionalExtension, params};

use super::reason::{DiscoverySource, LinkKind, NotCrawledReason, UrlState};
use crate::extract::compare::{Comparison, Origin, Origins};
use crate::extract::{Directive, PageData, Ref, RefSource, body, text};

pub const FORMAT: &str = "spiderlens-crawl";
/// 2: page data extraction (spec 005). 3 and later: the steps in
/// [`super::migrate::STEPS`] (spec 006).
pub const FORMAT_VERSION: i64 = super::migrate::LATEST;
/// Written to `meta.extraction_version` when a crawl collects page data.
pub const EXTRACTION_VERSION: &str = "1";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("crawl file error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("{0}")]
    Format(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// The file is damaged; part of it may be recoverable (FR-038).
    #[error("{0}")]
    Damaged(String),
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
    fetched_at INTEGER,
    response_ms INTEGER,
    size_bytes INTEGER,
    transfer_bytes INTEGER,
    resource_type TEXT,
    redirect_kind TEXT,
    noindex INTEGER,
    nofollow INTEGER,
    canonical TEXT,
    canonical_count INTEGER,
    canonical_conflict INTEGER
);
CREATE INDEX urls_queue ON urls(depth, queue_seq) WHERE state = 'queued';
CREATE INDEX urls_state ON urls(state, reason);
CREATE TABLE links (
    from_id INTEGER NOT NULL,
    to_id INTEGER,
    raw TEXT NOT NULL,
    kind TEXT NOT NULL,
    nofollow INTEGER NOT NULL,
    page_nofollow INTEGER NOT NULL,
    followed INTEGER NOT NULL,
    pos INTEGER,
    target TEXT,
    skip_reason TEXT,
    text TEXT,
    alt TEXT,
    rel TEXT,
    sponsored INTEGER NOT NULL DEFAULT 0,
    ugc INTEGER NOT NULL DEFAULT 0,
    source TEXT NOT NULL DEFAULT 'html',
    in_head INTEGER,
    noscript INTEGER NOT NULL DEFAULT 0,
    lang TEXT,
    width INTEGER,
    height INTEGER,
    srcset INTEGER NOT NULL DEFAULT 0,
    in_link INTEGER NOT NULL DEFAULT 0
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
CREATE TABLE pages (
    url_id INTEGER PRIMARY KEY,
    title_count INTEGER NOT NULL,
    meta_description TEXT,
    meta_description_count INTEGER NOT NULL,
    h1_1 TEXT,
    h1_2 TEXT,
    h1_count INTEGER NOT NULL,
    h2_1 TEXT,
    h2_2 TEXT,
    h2_count INTEGER NOT NULL,
    heading_counts TEXT NOT NULL,
    word_count INTEGER NOT NULL,
    lang TEXT,
    text_hash INTEGER,
    title_hash INTEGER,
    meta_description_hash INTEGER,
    meta_refresh_delay REAL,
    meta_refresh_url TEXT,
    partial INTEGER NOT NULL,
    encoding_errors INTEGER NOT NULL,
    encoding TEXT,
    extract_error TEXT,
    no_content INTEGER NOT NULL,
    html_stored INTEGER NOT NULL
);
CREATE TABLE page_text (
    url_id INTEGER NOT NULL,
    kind TEXT NOT NULL,
    level INTEGER,
    pos INTEGER NOT NULL,
    text TEXT NOT NULL,
    in_head INTEGER
);
CREATE INDEX page_text_url ON page_text(url_id, kind, pos);
CREATE TABLE directives (
    url_id INTEGER NOT NULL,
    pos INTEGER NOT NULL,
    directive TEXT NOT NULL,
    value TEXT,
    known INTEGER NOT NULL,
    source TEXT NOT NULL,
    crawler TEXT NOT NULL,
    in_head INTEGER
);
CREATE INDEX directives_url ON directives(url_id);
CREATE TABLE structured_data (
    url_id INTEGER NOT NULL,
    pos INTEGER NOT NULL,
    kind TEXT NOT NULL,
    raw TEXT NOT NULL,
    parseable INTEGER NOT NULL
);
CREATE INDEX structured_data_url ON structured_data(url_id);
CREATE TABLE bodies (
    url_id INTEGER PRIMARY KEY,
    codec TEXT NOT NULL,
    size INTEGER NOT NULL,
    body BLOB NOT NULL
);
CREATE TABLE headers (
    url_id INTEGER PRIMARY KEY,
    headers TEXT NOT NULL
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
    /// Spec 005 response facts (data-model.md, `urls`).
    pub response_ms: Option<u64>,
    pub size_bytes: Option<u64>,
    pub transfer_bytes: Option<u64>,
    pub resource_type: Option<&'static str>,
    pub redirect_kind: Option<&'static str>,
    pub noindex: Option<bool>,
    pub nofollow: Option<bool>,
    pub canonical: Option<String>,
    pub canonical_count: Option<u32>,
    pub canonical_conflict: Option<bool>,
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
    checkpointer: Option<Checkpointer>,
    /// Next render queue number; read from the file on first use.
    render_seq: Cell<Option<i64>>,
}

/// Copies the write-ahead log into the database file on its own thread, so
/// the crawl thread only appends to the log (spec 005 plan D9).
struct Checkpointer {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

const CHECKPOINT_EVERY: std::time::Duration = std::time::Duration::from_millis(250);

impl Drop for CrawlStore {
    fn drop(&mut self) {
        // The checkpoint connection closes first, so the crawl's own
        // connection is the last one and folds the log back on close.
        if let Some(mut c) = self.checkpointer.take() {
            c.stop.store(true, Ordering::Relaxed);
            if let Some(h) = c.handle.take() {
                let _ = h.join();
            }
        }
    }
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
    /// None for references that were not recorded as URLs.
    pub to: Option<i64>,
    pub raw: &'a str,
    pub kind: LinkKind,
    /// The link itself carries rel=nofollow.
    pub nofollow: bool,
    /// The page's meta robots or X-Robots-Tag says nofollow.
    pub page_nofollow: bool,
    pub followed: bool,
    /// Document order on the page.
    pub pos: Option<u32>,
    /// Resolved absolute URL when `to` is None.
    pub target: Option<&'a str>,
    /// Why the reference is not crawlable (FR-024).
    pub skip_reason: Option<&'a str>,
    /// Extracted context (spec 005); None for redirects.
    pub context: Option<&'a Ref>,
}

/// Extracted data for one HTML page, for [`CrawlStore::record_page`].
pub struct PageRecord<'a> {
    /// None when extraction failed.
    pub data: Option<&'a PageData>,
    pub extract_error: Option<&'a str>,
    /// Compressed body and its uncompressed size.
    pub html: Option<(&'a [u8], usize)>,
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
        super::migrate::apply_all(&conn)?;
        let store = Self {
            conn,
            next_seq: 1,
            live: Cell::default(),
            checkpointer: None,
            render_seq: Cell::new(None),
        };
        store.set_meta("format", FORMAT)?;
        store.set_meta("extraction_version", EXTRACTION_VERSION)?;
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
        let version = super::migrate::version(&conn)?;
        if version > FORMAT_VERSION {
            return Err(StoreError::Format(format!(
                "{} was written by a newer SpiderLens (crawl format {version}); update SpiderLens to change it",
                path.display()
            )));
        }
        let max_seq: i64 =
            conn.query_row("SELECT COALESCE(MAX(queue_seq), 0) FROM urls", [], |r| r.get(0))?;
        let store = Self {
            conn,
            next_seq: max_seq + 1,
            live: Cell::default(),
            checkpointer: None,
            render_seq: Cell::new(None),
        };
        let mut live = StateCounts::default();
        for (state, n) in store.state_counts()? {
            *live.slot(state) = n;
        }
        store.live.set(live);
        Ok(store)
    }

    /// Opens a crawl file for reading only: nothing is written, not even the
    /// journal mode, so viewing never changes a file (spec 006 FR-036) and
    /// read-only files and newer formats can be shown (FR-021, FR-037).
    pub fn open_read_only(path: &Path) -> Result<Self> {
        let conn = connect_read_only(path)?;
        let format: Option<String> = conn
            .query_row("SELECT value FROM meta WHERE key = 'format'", [], |r| r.get(0))
            .optional()
            .map_err(|e| damaged(path, e))?;
        if format.as_deref() != Some(FORMAT) {
            return Err(StoreError::Format(format!(
                "{} is not a SpiderLens crawl file",
                path.display()
            )));
        }
        let compat = super::migrate::read_compat(&conn)?;
        if compat > FORMAT_VERSION {
            return Err(StoreError::Format(format!(
                "{} was written by a newer SpiderLens (crawl format {}); update SpiderLens to open it",
                path.display(),
                super::migrate::version(&conn)?
            )));
        }
        let store = Self {
            conn,
            next_seq: 0,
            live: Cell::default(),
            checkpointer: None,
            render_seq: Cell::new(None),
        };
        let mut live = StateCounts::default();
        for (state, n) in store.state_counts()? {
            *live.slot(state) = n;
        }
        store.live.set(live);
        Ok(store)
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    /// The connection, for tests that need to set up unusual files.
    #[doc(hidden)]
    pub fn conn_for_tests(&self) -> &Connection {
        &self.conn
    }

    /// Where the file is, when it is a file.
    pub fn path(&self) -> Option<std::path::PathBuf> {
        self.conn
            .path()
            .filter(|p| !p.is_empty())
            .map(std::path::PathBuf::from)
    }

    /// Records an event in the file's history (spec 006 data-model.md).
    /// Files without the `events` table (format 2, viewed only) skip it.
    pub fn record_event(&self, kind: &str, detail: Option<&str>) -> Result<()> {
        if super::migrate::version(&self.conn)? < 3 {
            return Ok(());
        }
        self.conn
            .prepare_cached(
                "INSERT INTO events (at, kind, app_version, os, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?
            .execute(params![
                jiff::Timestamp::now().as_millisecond(),
                kind,
                APP_VERSION,
                os_name(),
                detail
            ])?;
        Ok(())
    }

    /// Writes the crawl's last committed state into the database file alone
    /// (empties the write-ahead log), so the file can be copied. False when
    /// a reader still holds part of the log.
    pub(crate) fn checkpoint_truncate(&self) -> Result<bool> {
        let (busy, log, done): (i64, i64, i64) =
            self.conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?;
        Ok(busy == 0 && log == done)
    }

    /// Moves the write connection to another file holding the same data
    /// (Save As while crawling, plan D5): the old connection closes, the new
    /// one is configured the same way and keeps its own checkpointer.
    pub(crate) fn reopen_at(&mut self, path: &Path) -> Result<()> {
        let background = self.checkpointer.is_some();
        if let Some(mut c) = self.checkpointer.take() {
            c.stop.store(true, Ordering::Relaxed);
            if let Some(h) = c.handle.take() {
                let _ = h.join();
            }
        }
        let conn = Connection::open(path)?;
        Self::configure(&conn)?;
        drop(std::mem::replace(&mut self.conn, conn));
        if background {
            self.checkpoint_in_background()?;
        }
        Ok(())
    }

    /// Hands write-ahead-log checkpoints to a background thread with its own
    /// connection. Used by a running crawl; with it, the crawl thread's
    /// commits only append to the log. Passive checkpoints never block the
    /// writer.
    pub fn checkpoint_in_background(&mut self) -> Result<()> {
        if self.checkpointer.is_some() {
            return Ok(());
        }
        let Some(path) = self
            .conn
            .path()
            .filter(|p| !p.is_empty())
            .map(std::path::PathBuf::from)
        else {
            return Ok(());
        };
        let other = Connection::open(&path)?;
        self.conn.pragma_update(None, "wal_autocheckpoint", 0)?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let handle = std::thread::Builder::new()
            .name("spiderlens-checkpoint".into())
            .spawn(move || {
                while !flag.load(Ordering::Relaxed) {
                    std::thread::sleep(CHECKPOINT_EVERY);
                    let _ = other.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |_| Ok(()));
                }
            })
            .map_err(|e| StoreError::Format(format!("could not start the checkpoint thread: {e}")))?;
        self.checkpointer = Some(Checkpointer {
            stop,
            handle: Some(handle),
        });
        Ok(())
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
        if let Ok(v) = std::env::var("SL_CACHE") {
            conn.pragma_update(None, "cache_size", v.as_str())?;
        }
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

    /// Builds the indexes the Duplicate filters read (spec 005 data model).
    /// Their keys are random hashes, so keeping them up to date during a
    /// crawl dirties a page of each index per insert and costs a fifth of
    /// the crawl's speed; one sort at the end is far cheaper. Called when a
    /// run ends (completed, paused or stopped).
    pub fn build_result_indexes(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS pages_title_hash ON pages(title_hash) WHERE title_hash IS NOT NULL;
             CREATE INDEX IF NOT EXISTS pages_md_hash ON pages(meta_description_hash)
                 WHERE meta_description_hash IS NOT NULL;",
        )?;
        Ok(())
    }

    /// Drops the indexes [`Self::build_result_indexes`] made, so a resumed
    /// run writes as fast as a new one.
    pub fn drop_result_indexes(&self) -> Result<()> {
        self.conn
            .execute_batch("DROP INDEX IF EXISTS pages_title_hash; DROP INDEX IF EXISTS pages_md_hash;")?;
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
        // Most references carry only anchor text; they take a narrow insert,
        // which costs the crawl thread far less per row (plan D9).
        let plain = link.context.is_none_or(|c| {
            c.alt.is_none()
                && c.rel.is_none()
                && c.source == RefSource::Html
                && c.in_head.is_none()
                && !c.noscript
                && c.lang.is_none()
                && c.width.is_none()
                && c.height.is_none()
                && !c.srcset
                && !c.in_link
        }) && link.target.is_none();
        if plain {
            self.conn
                .prepare_cached(
                    "INSERT INTO links (from_id, to_id, raw, kind, nofollow, page_nofollow, followed, pos, skip_reason,
                     text) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                )?
                .execute(params![
                    link.from,
                    link.to,
                    link.raw,
                    link.kind.as_str(),
                    link.nofollow,
                    link.page_nofollow,
                    link.followed,
                    link.pos,
                    link.skip_reason,
                    link.context.and_then(|c| c.text.as_deref()),
                ])?;
            return Ok(());
        }
        let c = link.context;
        self.conn
            .prepare_cached(
                "INSERT INTO links (from_id, to_id, raw, kind, nofollow, page_nofollow, followed, pos, target,
                 skip_reason, text, alt, rel, sponsored, ugc, source, in_head, noscript, lang, width, height,
                 srcset, in_link)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19,
                 ?20, ?21, ?22, ?23)",
            )?
            .execute(params![
                link.from,
                link.to,
                link.raw,
                link.kind.as_str(),
                link.nofollow,
                link.page_nofollow,
                link.followed,
                link.pos,
                link.target,
                link.skip_reason,
                c.and_then(|c| c.text.as_deref()),
                c.and_then(|c| c.alt.as_deref()),
                c.and_then(|c| c.rel.as_deref()),
                c.is_some_and(|c| c.sponsored),
                c.is_some_and(|c| c.ugc),
                c.map_or("html", |c| c.source.as_str()),
                c.and_then(|c| c.in_head),
                c.is_some_and(|c| c.noscript),
                c.and_then(|c| c.lang.as_deref()),
                c.and_then(|c| c.width),
                c.and_then(|c| c.height),
                c.is_some_and(|c| c.srcset),
                c.is_some_and(|c| c.in_link),
            ])?;
        Ok(())
    }

    /// Stores what was extracted from one HTML page (spec 005,
    /// data-model.md): the `pages` row, every title, description and
    /// heading, meta directives, structured data and the raw HTML.
    pub fn record_page(&self, id: i64, page: &PageRecord<'_>) -> Result<()> {
        let empty;
        let d = match page.data {
            Some(d) => d,
            None => {
                empty = PageData::failed();
                &empty
            }
        };
        let h = |level: u8, n: usize| {
            d.headings
                .iter()
                .filter(|x| x.level == level)
                .nth(n)
                .map(|x| x.text.as_str())
        };
        let count = |level: u8| d.headings.iter().filter(|x| x.level == level).count() as i64;
        let heading_counts = (1..=6)
            .map(|l| count(l).to_string())
            .collect::<Vec<_>>()
            .join(",");
        self.conn
            .prepare_cached(
                "INSERT OR REPLACE INTO pages (url_id, title_count, meta_description, meta_description_count,
                 h1_1, h1_2, h1_count, h2_1, h2_2, h2_count, heading_counts, word_count, lang, text_hash,
                 title_hash, meta_description_hash, meta_refresh_delay, meta_refresh_url, partial,
                 encoding_errors, encoding, extract_error, no_content, html_stored)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                 ?19, ?20, ?21, ?22, ?23, ?24)",
            )?
            .execute(params![
                id,
                d.titles.len() as i64,
                d.meta_description(),
                d.meta_descriptions.len() as i64,
                h(1, 0),
                h(1, 1),
                count(1),
                h(2, 0),
                h(2, 1),
                count(2),
                heading_counts,
                d.word_count,
                d.lang,
                page.data.map(|d| d.text_hash),
                d.title().map(text::hash),
                d.meta_description().map(text::hash),
                d.meta_refresh.as_ref().map(|r| r.delay),
                d.meta_refresh.as_ref().and_then(|r| r.url.as_deref()),
                d.partial,
                d.encoding_errors,
                page.data.map(|d| d.encoding),
                page.extract_error,
                d.no_content,
                page.html.is_some(),
            ])?;
        let mut text_row = self.conn.prepare_cached(
            "INSERT INTO page_text (url_id, kind, level, pos, text, in_head) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for (pos, t) in d.titles.iter().enumerate() {
            text_row.execute(params![id, "title", None::<u8>, pos as i64, t.text, t.in_head])?;
        }
        for (pos, t) in d.meta_descriptions.iter().enumerate() {
            text_row.execute(params![
                id,
                "meta_description",
                None::<u8>,
                pos as i64,
                t.text,
                t.in_head
            ])?;
        }
        for (pos, x) in d.headings.iter().enumerate() {
            text_row.execute(params![id, "heading", x.level, pos as i64, x.text, None::<bool>])?;
        }
        self.record_directives(id, &d.directives, 0)?;
        let mut sd = self.conn.prepare_cached(
            "INSERT INTO structured_data (url_id, pos, kind, raw, parseable) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for (pos, b) in d.structured.iter().enumerate() {
            sd.execute(params![id, pos as i64, b.kind.as_str(), b.raw, b.parseable])?;
        }
        if let Some((packed, size)) = page.html {
            self.conn
                .prepare_cached(
                    "INSERT OR REPLACE INTO bodies (url_id, codec, size, body) VALUES (?1, ?2, ?3, ?4)",
                )?
                .execute(params![id, body::CODEC, size as i64, packed])?;
        }
        Ok(())
    }

    /// Robots directives of one resource, numbered from `first_pos`.
    pub fn record_directives(&self, id: i64, directives: &[Directive], first_pos: usize) -> Result<()> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO directives (url_id, pos, directive, value, known, source, crawler, in_head)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        for (i, d) in directives.iter().enumerate() {
            stmt.execute(params![
                id,
                (first_pos + i) as i64,
                d.directive,
                d.value,
                d.known,
                d.source.as_str(),
                d.crawler,
                d.in_head
            ])?;
        }
        Ok(())
    }

    /// Response headers of one fetched URL, as a JSON array of pairs.
    pub fn record_headers(&self, id: i64, headers: &[(String, String)]) -> Result<()> {
        self.conn
            .prepare_cached("INSERT OR REPLACE INTO headers (url_id, headers) VALUES (?1, ?2)")?
            .execute(params![
                id,
                serde_json::to_string(headers).expect("pairs serialize")
            ])?;
        Ok(())
    }

    /// The stored raw HTML of a page, byte-identical to the received body
    /// (FR-030). None when the crawl did not keep it.
    pub fn page_html(&self, url_id: i64) -> Result<Option<Vec<u8>>> {
        let row: Option<(String, i64, Vec<u8>)> = self
            .conn
            .prepare_cached("SELECT codec, size, body FROM bodies WHERE url_id = ?1")?
            .query_row([url_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?;
        let Some((codec, size, packed)) = row else {
            return Ok(None);
        };
        if codec != body::CODEC {
            return Err(StoreError::Format(format!("unknown body codec {codec}")));
        }
        body::decompress(&packed, size as usize)
            .map(Some)
            .map_err(|e| StoreError::Format(format!("stored HTML is damaged: {e}")))
    }

    /// Everything stored for one URL, as JSON (`spiderlens page`, tests).
    /// None when the URL is not in the file.
    pub fn page_report(&self, url: &str) -> Result<Option<serde_json::Value>> {
        use serde_json::{Value, json};
        let Some(id) = self.id_of(url)? else {
            return Ok(None);
        };
        let one = |sql: &str| -> Result<Value> {
            Ok(self.rows_json(sql, id)?.into_iter().next().unwrap_or(Value::Null))
        };
        let mut response = one(
            "SELECT u.url, u.state, u.status, u.content_type, u.resource_type, u.response_ms, u.size_bytes,
             u.transfer_bytes, t.url AS redirect_to, u.redirect_kind, u.error_class, u.error_message, u.title,
             u.noindex, u.nofollow, u.canonical, u.canonical_count, u.canonical_conflict, u.depth, u.internal
             FROM urls u LEFT JOIN urls t ON t.id = u.redirect_to WHERE u.id = ?1",
        )?;
        let page = one("SELECT * FROM pages WHERE url_id = ?1")?;
        let texts = self.rows_json(
            "SELECT kind, level, text, in_head FROM page_text WHERE url_id = ?1 ORDER BY kind, pos",
            id,
        )?;
        let pick = |kind: &str| -> Vec<Value> {
            texts
                .iter()
                .filter(|t| t["kind"] == kind)
                .map(|t| {
                    let mut t = t.clone();
                    let o = t.as_object_mut().expect("rows are objects");
                    o.remove("kind");
                    if kind != "heading" {
                        o.remove("level");
                    } else {
                        o.remove("in_head");
                    }
                    t
                })
                .collect()
        };
        let headers: Value = self
            .conn
            .prepare_cached("SELECT headers FROM headers WHERE url_id = ?1")?
            .query_row([id], |r| r.get::<_, String>(0))
            .optional()?
            .and_then(|h| serde_json::from_str(&h).ok())
            .unwrap_or(Value::Null);
        let obj = response.as_object_mut().expect("the URL row exists");
        obj.insert("page".into(), page);
        obj.insert("titles".into(), Value::Array(pick("title")));
        obj.insert("meta_descriptions".into(), Value::Array(pick("meta_description")));
        obj.insert("headings".into(), Value::Array(pick("heading")));
        obj.insert(
            "directives".into(),
            Value::Array(self.rows_json(
                "SELECT directive, value, known, source, crawler, in_head FROM directives
                 WHERE url_id = ?1 ORDER BY pos",
                id,
            )?),
        );
        obj.insert(
            "links".into(),
            Value::Array(self.rows_json(
                "SELECT l.kind, l.raw, t.url, l.target, l.skip_reason, l.text, l.alt, l.rel, l.nofollow,
                 l.sponsored, l.ugc, l.page_nofollow, l.followed, l.source, l.in_head, l.noscript, l.lang,
                 l.width, l.height, l.srcset, l.in_link, t.internal
                 FROM links l LEFT JOIN urls t ON t.id = l.to_id WHERE l.from_id = ?1 ORDER BY l.rowid",
                id,
            )?),
        );
        obj.insert(
            "structured_data".into(),
            Value::Array(self.rows_json(
                "SELECT kind, raw, parseable FROM structured_data WHERE url_id = ?1 ORDER BY pos",
                id,
            )?),
        );
        obj.insert("headers".into(), headers);
        let stored: bool = self
            .conn
            .prepare_cached("SELECT EXISTS (SELECT 1 FROM bodies WHERE url_id = ?1)")?
            .query_row([id], |r| r.get(0))?;
        obj.insert("html_stored".into(), json!(stored));
        Ok(Some(response))
    }

    /// Runs a query with one id parameter; each row becomes a JSON object
    /// keyed by column name.
    fn rows_json(&self, sql: &str, id: i64) -> Result<Vec<serde_json::Value>> {
        use rusqlite::types::ValueRef;
        use serde_json::{Map, Value, json};
        let mut stmt = self.conn.prepare_cached(sql)?;
        let names: Vec<String> = stmt.column_names().iter().map(|n| n.to_string()).collect();
        let rows = stmt.query_map([id], |r| {
            let mut m = Map::new();
            for (i, n) in names.iter().enumerate() {
                let v = match r.get_ref(i)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(x) => json!(x),
                    ValueRef::Real(x) => json!(x),
                    ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                    ValueRef::Blob(b) => json!(format!("<{} bytes>", b.len())),
                };
                m.insert(n.clone(), v);
            }
            Ok(Value::Object(m))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// The row id of a URL.
    pub fn id_of(&self, url: &str) -> Result<Option<i64>> {
        Ok(self
            .conn
            .prepare_cached("SELECT id FROM urls WHERE url = ?1")?
            .query_row([url], |r| r.get(0))
            .optional()?)
    }

    /// The crawl file's format version.
    pub fn format_version(&self) -> Result<i64> {
        Ok(self
            .meta("format_version")?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0))
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
                 error_class = ?7, error_message = ?8, robots_rule = COALESCE(?9, robots_rule), fetched_at = ?10,
                 response_ms = ?11, size_bytes = ?12, transfer_bytes = ?13, resource_type = ?14,
                 redirect_kind = ?15, noindex = ?16, nofollow = ?17, canonical = ?18, canonical_count = ?19,
                 canonical_conflict = ?20
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
                r.response_ms.map(|v| v as i64),
                r.size_bytes.map(|v| v as i64),
                r.transfer_bytes.map(|v| v as i64),
                r.resource_type,
                r.redirect_kind,
                r.noindex,
                r.nofollow,
                r.canonical,
                r.canonical_count,
                r.canonical_conflict,
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

    /// Recovery: rows left in flight by a crash go back to the queue.
    pub fn requeue_in_flight(&self) -> Result<usize> {
        let a = self
            .conn
            .execute("UPDATE urls SET state = 'queued' WHERE state = 'in_flight'", [])?;
        self.shift(Some(UrlState::InFlight), UrlState::Queued, a);
        Ok(a)
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

    /// Crawled URLs that failed, per error class.
    pub fn error_classes(&self) -> Result<Vec<(String, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT error_class, COUNT(*) FROM urls WHERE error_class IS NOT NULL GROUP BY error_class ORDER BY error_class",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
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

/// Written to `meta.render_version` when a crawl renders (spec 014).
pub const RENDER_VERSION: &str = "1";

/// Render state of one page (spec 014 data-model.md, `renders.state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderStatus {
    Queued,
    Rendering,
    Rendered,
    TimedOut,
    Failed,
    NotRendered,
}

impl RenderStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Rendering => "rendering",
            Self::Rendered => "rendered",
            Self::TimedOut => "timed_out",
            Self::Failed => "failed",
            Self::NotRendered => "not_rendered",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "queued" => Self::Queued,
            "rendering" => Self::Rendering,
            "rendered" => Self::Rendered,
            "timed_out" => Self::TimedOut,
            "failed" => Self::Failed,
            "not_rendered" => Self::NotRendered,
            _ => return None,
        })
    }
}

/// One page waiting in the render queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderEntry {
    pub url_id: i64,
    pub url: String,
    pub host_key: String,
    pub seq: i64,
    pub attempts: u32,
}

/// One `render_resources` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRow<'a> {
    pub url: &'a str,
    /// Lowercase CDP resource type.
    pub kind: &'a str,
    pub host: &'a str,
    pub third_party: bool,
    /// `loaded`, `cached`, `failed`, `blocked_setting`, `blocked_robots`.
    pub outcome: &'a str,
    pub status: Option<u16>,
    pub error: Option<&'a str>,
    pub robots_rule: Option<&'a str>,
    pub bytes: u64,
}

/// One `console_entries` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleRow<'a> {
    /// `error`, `warning`, `exception`.
    pub level: &'a str,
    pub message: &'a str,
    pub source: Option<(&'a str, u32, u32)>,
}

/// What rendering produced for a page that has a rendered version.
pub struct RenderedPageRecord<'a> {
    /// Rendered extraction; None when it failed.
    pub data: Option<&'a PageData>,
    pub extract_error: Option<&'a str>,
    /// Raw extraction, for the rows of elements gone after rendering.
    pub raw: Option<&'a PageData>,
    pub comparison: Option<&'a Comparison>,
    /// Rendered meta directives combined with the header directives.
    pub noindex: bool,
    pub nofollow: bool,
    pub canonical: Option<&'a str>,
    pub canonical_count: u32,
    pub canonical_conflict: bool,
    /// Compressed serialised DOM and its size, when HTML is kept.
    pub html: Option<(&'a [u8], usize)>,
    /// Raw `links.pos` of references gone after rendering.
    pub raw_only_links: &'a [u32],
}

/// The final result of one render, for [`CrawlStore::record_render`].
pub struct RenderRecord<'a> {
    pub status: RenderStatus,
    pub reason: Option<&'a str>,
    pub reason_detail: Option<&'a str>,
    pub render_ms: Option<u32>,
    pub window: Option<(u32, u32)>,
    pub js_redirect: Option<&'a str>,
    pub js_redirect_id: Option<i64>,
    /// Errors and warnings in total, including those over the stored limit.
    pub console_totals: (u32, u32),
    pub resources_failed: u32,
    pub resources_blocked: u32,
    pub bot_challenge: bool,
    pub rendered_at: i64,
    pub page: Option<RenderedPageRecord<'a>>,
    /// At most 500 are stored.
    pub resources: &'a [ResourceRow<'a>],
    /// At most 50 are stored.
    pub console: &'a [ConsoleRow<'a>],
    /// WebP bytes, width and height.
    pub screenshot: Option<(&'a [u8], u32, u32)>,
}

/// Rows of one page kept in `render_resources`.
pub const MAX_RESOURCE_ROWS: usize = 500;
/// Rows of one page kept in `console_entries`.
pub const MAX_CONSOLE_ROWS: usize = 50;

fn origin_at(o: Option<&Origins>, i: usize) -> &'static str {
    o.and_then(|o| o.rendered.get(i))
        .copied()
        .unwrap_or(Origin::Rendered)
        .as_str()
}

/// Spec 014: the render queue and the rendered version of pages. Every table
/// here comes from migration step 4 and stays empty when a crawl does not
/// render.
impl CrawlStore {
    fn take_render_seq(&self) -> Result<i64> {
        let next = match self.render_seq.get() {
            Some(n) => n,
            None => {
                self.conn
                    .query_row("SELECT COALESCE(MAX(queue_seq), 0) FROM renders", [], |r| {
                        r.get::<_, i64>(0)
                    })?
                    + 1
            }
        };
        self.render_seq.set(Some(next + 1));
        Ok(next)
    }

    /// Queues a page for rendering. `input` is the compressed raw body and
    /// its size, kept only until the render is final, when the crawl does
    /// not store page HTML (research R8).
    pub fn enqueue_render(&self, url_id: i64, input: Option<(&[u8], usize)>) -> Result<()> {
        let seq = self.take_render_seq()?;
        self.conn
            .prepare_cached(
                "INSERT INTO renders (url_id, state, queue_seq) VALUES (?1, 'queued', ?2)
                 ON CONFLICT(url_id) DO NOTHING",
            )?
            .execute(params![url_id, seq])?;
        if let Some((packed, size)) = input {
            self.conn
                .prepare_cached(
                    "INSERT OR REPLACE INTO render_inputs (url_id, codec, size, body) VALUES (?1, ?2, ?3, ?4)",
                )?
                .execute(params![url_id, body::CODEC, size as i64, packed])?;
        }
        Ok(())
    }

    /// Records a page that is not rendered at all (`outside_rules`,
    /// `html_not_stored`, `browser_unavailable`).
    pub fn skip_render(&self, url_id: i64, reason: &str, detail: Option<&str>) -> Result<()> {
        self.conn
            .prepare_cached(
                "INSERT INTO renders (url_id, state, reason, reason_detail) VALUES (?1, 'not_rendered', ?2, ?3)
                 ON CONFLICT(url_id) DO UPDATE SET state = 'not_rendered', reason = ?2, reason_detail = ?3,
                 queue_seq = NULL",
            )?
            .execute(params![url_id, reason, detail])?;
        self.conn
            .prepare_cached("DELETE FROM render_inputs WHERE url_id = ?1")?
            .execute([url_id])?;
        Ok(())
    }

    /// Up to `limit` queued renders after `after`, in queue order.
    pub fn renders_after(&self, after: Option<i64>, limit: usize) -> Result<Vec<RenderEntry>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT r.url_id, u.url, u.host_key, r.queue_seq, r.attempts FROM renders r
             JOIN urls u ON u.id = r.url_id
             WHERE r.state = 'queued' AND r.queue_seq > ?1 ORDER BY r.queue_seq LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![after.unwrap_or(i64::MIN), limit as i64], |r| {
            Ok(RenderEntry {
                url_id: r.get(0)?,
                url: r.get(1)?,
                host_key: r.get(2)?,
                seq: r.get(3)?,
                attempts: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// A queued render starts: `queued → rendering`, one more attempt.
    pub fn mark_rendering(&self, url_id: i64) -> Result<bool> {
        let n = self
            .conn
            .prepare_cached(
                "UPDATE renders SET state = 'rendering', attempts = attempts + 1
                 WHERE url_id = ?1 AND state = 'queued'",
            )?
            .execute([url_id])?;
        Ok(n == 1)
    }

    /// A render to try again (after a crash): `rendering → queued`, keeping
    /// its place in the queue.
    pub fn requeue_render(&self, url_id: i64) -> Result<()> {
        self.conn
            .prepare_cached("UPDATE renders SET state = 'queued' WHERE url_id = ?1 AND state = 'rendering'")?
            .execute([url_id])?;
        Ok(())
    }

    /// Resume: renders left running by a pause or a crash, and renders the
    /// crawl stopped, go back to the queue. Returns how many.
    pub fn requeue_rendering(&self) -> Result<usize> {
        if !self.has_render_tables()? {
            return Ok(0);
        }
        let a = self.conn.execute(
            "UPDATE renders SET state = 'queued' WHERE state = 'rendering'",
            [],
        )?;
        let b = self.conn.execute(
            "UPDATE renders SET state = 'queued', reason = NULL, reason_detail = NULL
             WHERE state = 'not_rendered' AND reason = 'crawl_stopped' AND queue_seq IS NOT NULL",
            [],
        )?;
        Ok(a + b)
    }

    /// Stop: every queued render becomes "Crawl stopped". Their queue place
    /// and input are kept so a resume can render them.
    pub fn stop_renders(&self) -> Result<usize> {
        if !self.has_render_tables()? {
            return Ok(0);
        }
        Ok(self.conn.execute(
            "UPDATE renders SET state = 'not_rendered', reason = 'crawl_stopped'
             WHERE state IN ('queued', 'rendering')",
            [],
        )?)
    }

    /// Whether the file has the rendering tables (format 4 and later).
    fn has_render_tables(&self) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'renders')",
            [],
            |r| r.get(0),
        )?)
    }

    /// Renders per state, for progress and the summary.
    pub fn render_counts(&self) -> Result<Vec<(RenderStatus, u64)>> {
        if !self.has_render_tables()? {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare_cached("SELECT state, COUNT(*) FROM renders GROUP BY state")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (s, n) = row?;
            if let Some(s) = RenderStatus::parse(&s) {
                out.push((s, n as u64));
            }
        }
        Ok(out)
    }

    /// The raw HTML a render starts from: the temporary input, or the stored
    /// page HTML.
    pub fn render_input(&self, url_id: i64) -> Result<Option<Vec<u8>>> {
        let row: Option<(String, i64, Vec<u8>)> = self
            .conn
            .prepare_cached("SELECT codec, size, body FROM render_inputs WHERE url_id = ?1")?
            .query_row([url_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?;
        match row {
            Some((codec, size, packed)) => unpack(&codec, size, &packed).map(Some),
            None => self.page_html(url_id),
        }
    }

    /// The serialised DOM of a rendered page, when HTML was kept (FR-032).
    pub fn rendered_html(&self, url_id: i64) -> Result<Option<Vec<u8>>> {
        if !self.has_render_tables()? {
            return Ok(None);
        }
        let row: Option<(String, i64, Vec<u8>)> = self
            .conn
            .prepare_cached("SELECT codec, size, body FROM rendered_bodies WHERE url_id = ?1")?
            .query_row([url_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?;
        row.map(|(codec, size, packed)| unpack(&codec, size, &packed))
            .transpose()
    }

    /// The screenshot of a rendered page: WebP bytes, width, height (FR-033).
    pub fn screenshot(&self, url_id: i64) -> Result<Option<(Vec<u8>, u32, u32)>> {
        if !self.has_render_tables()? {
            return Ok(None);
        }
        Ok(self
            .conn
            .prepare_cached("SELECT image, width, height FROM screenshots WHERE url_id = ?1")?
            .query_row([url_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?)
    }

    /// Records the browser in `meta.renderer` (FR-003) and marks the file
    /// as rendered (`meta.render_version`).
    pub fn set_renderer(&self, renderer_json: &str) -> Result<()> {
        self.set_meta("renderer", renderer_json)?;
        self.set_meta("render_version", RENDER_VERSION)
    }

    /// Adds feature ids to `meta.data_features` (spec 007 plan D3), once.
    pub fn add_data_features(&self, features: &[&str]) -> Result<()> {
        let mut list: Vec<String> = self
            .meta("data_features")?
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|f| !f.is_empty())
            .map(String::from)
            .collect();
        for f in features {
            if !list.iter().any(|x| x == f) {
                list.push((*f).to_string());
            }
        }
        self.set_meta("data_features", &list.join(","))
    }

    /// A reference found only in the rendered page (through `discover()`),
    /// with `rpos`, its position in the rendered document.
    pub fn add_rendered_link(&self, link: &NewLink, rpos: u32) -> Result<()> {
        let c = link.context;
        self.conn
            .prepare_cached(
                "INSERT INTO rendered_links (from_id, to_id, raw, kind, nofollow, page_nofollow, followed, pos,
                 target, skip_reason, text, alt, rel, sponsored, ugc, source, in_head, noscript, lang, width,
                 height, srcset, in_link, rpos)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19,
                 ?20, ?21, ?22, ?23, ?24)",
            )?
            .execute(params![
                link.from,
                link.to,
                link.raw,
                link.kind.as_str(),
                link.nofollow,
                link.page_nofollow,
                link.followed,
                link.pos,
                link.target,
                link.skip_reason,
                c.and_then(|c| c.text.as_deref()),
                c.and_then(|c| c.alt.as_deref()),
                c.and_then(|c| c.rel.as_deref()),
                c.is_some_and(|c| c.sponsored),
                c.is_some_and(|c| c.ugc),
                c.map_or("html", |c| c.source.as_str()),
                c.and_then(|c| c.in_head),
                c.is_some_and(|c| c.noscript),
                c.and_then(|c| c.lang.as_deref()),
                c.and_then(|c| c.width),
                c.and_then(|c| c.height),
                c.is_some_and(|c| c.srcset),
                c.is_some_and(|c| c.in_link),
                rpos,
            ])?;
        Ok(())
    }

    /// Stores the final result of one render (data-model.md "Write path"):
    /// the `renders` row and, for a rendered page, every rendered row. The
    /// temporary input is dropped.
    pub fn record_render(&self, url_id: i64, r: &RenderRecord<'_>) -> Result<()> {
        self.conn
            .prepare_cached(
                "UPDATE renders SET state = ?2, reason = ?3, reason_detail = ?4, queue_seq = NULL,
                 render_ms = ?5, window_w = ?6, window_h = ?7, js_redirect = ?8, js_redirect_id = ?9,
                 console_errors = ?10, console_warnings = ?11, resources_failed = ?12,
                 resources_blocked = ?13, bot_challenge = ?14, rendered_at = ?15
                 WHERE url_id = ?1",
            )?
            .execute(params![
                url_id,
                r.status.as_str(),
                r.reason,
                r.reason_detail,
                r.render_ms,
                r.window.map(|w| w.0),
                r.window.map(|w| w.1),
                r.js_redirect,
                r.js_redirect_id,
                r.console_totals.0,
                r.console_totals.1,
                r.resources_failed,
                r.resources_blocked,
                r.bot_challenge,
                r.rendered_at,
            ])?;
        if let Some(p) = &r.page {
            self.record_rendered_page(url_id, p)?;
        }
        let mut res = self.conn.prepare_cached(
            "INSERT INTO render_resources (url_id, pos, url, type, host, third_party, outcome, status, error,
             robots_rule, bytes) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )?;
        for (pos, x) in r.resources.iter().take(MAX_RESOURCE_ROWS).enumerate() {
            res.execute(params![
                url_id,
                pos as i64,
                x.url,
                x.kind,
                x.host,
                x.third_party,
                x.outcome,
                x.status,
                x.error,
                x.robots_rule,
                x.bytes as i64,
            ])?;
        }
        let mut con = self.conn.prepare_cached(
            "INSERT INTO console_entries (url_id, pos, level, message, source_url, line, col)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        for (pos, c) in r.console.iter().take(MAX_CONSOLE_ROWS).enumerate() {
            con.execute(params![
                url_id,
                pos as i64,
                c.level,
                c.message,
                c.source.map(|s| s.0),
                c.source.map(|s| s.1),
                c.source.map(|s| s.2),
            ])?;
        }
        if let Some((image, w, h)) = r.screenshot {
            self.conn
                .prepare_cached(
                    "INSERT OR REPLACE INTO screenshots (url_id, format, width, height, image)
                     VALUES (?1, 'webp', ?2, ?3, ?4)",
                )?
                .execute(params![url_id, w, h, image])?;
        }
        self.conn
            .prepare_cached("DELETE FROM render_inputs WHERE url_id = ?1")?
            .execute([url_id])?;
        Ok(())
    }

    fn record_rendered_page(&self, id: i64, p: &RenderedPageRecord<'_>) -> Result<()> {
        let empty;
        let d = match p.data {
            Some(d) => d,
            None => {
                empty = PageData::failed();
                &empty
            }
        };
        let h = |level: u8, n: usize| {
            d.headings
                .iter()
                .filter(|x| x.level == level)
                .nth(n)
                .map(|x| x.text.as_str())
        };
        let count = |level: u8| d.headings.iter().filter(|x| x.level == level).count() as i64;
        let heading_counts = (1..=6)
            .map(|l| count(l).to_string())
            .collect::<Vec<_>>()
            .join(",");
        self.conn
            .prepare_cached(
                "INSERT OR REPLACE INTO rendered_pages (url_id, title_count, meta_description,
                 meta_description_count, h1_1, h1_2, h1_count, h2_1, h2_2, h2_count, heading_counts,
                 word_count, lang, text_hash, title_hash, meta_description_hash, meta_refresh_delay,
                 meta_refresh_url, extract_error, no_content, title, noindex, nofollow, canonical,
                 canonical_count, canonical_conflict, rendered_html_stored)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                 ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27)",
            )?
            .execute(params![
                id,
                d.titles.len() as i64,
                d.meta_description(),
                d.meta_descriptions.len() as i64,
                h(1, 0),
                h(1, 1),
                count(1),
                h(2, 0),
                h(2, 1),
                count(2),
                heading_counts,
                d.word_count,
                d.lang,
                p.data.map(|d| d.text_hash),
                d.title().map(text::hash),
                d.meta_description().map(text::hash),
                d.meta_refresh.as_ref().map(|r| r.delay),
                d.meta_refresh.as_ref().and_then(|r| r.url.as_deref()),
                p.extract_error,
                d.no_content,
                d.title(),
                p.noindex,
                p.nofollow,
                p.canonical,
                p.canonical_count,
                p.canonical_conflict,
                p.html.is_some(),
            ])?;
        let o = p.comparison.map(|c| &c.origins);
        let mut text_row = self.conn.prepare_cached(
            "INSERT INTO rendered_text (url_id, kind, level, pos, text, in_head, origin)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        for (pos, t) in d.titles.iter().enumerate() {
            let origin = origin_at(o.map(|o| &o.titles), pos);
            text_row.execute(params![
                id, "title", None::<u8>, pos as i64, t.text, t.in_head, origin
            ])?;
        }
        for (pos, t) in d.meta_descriptions.iter().enumerate() {
            let origin = origin_at(o.map(|o| &o.descriptions), pos);
            text_row.execute(params![
                id,
                "meta_description",
                None::<u8>,
                pos as i64,
                t.text,
                t.in_head,
                origin
            ])?;
        }
        for (pos, x) in d.headings.iter().enumerate() {
            let origin = origin_at(o.map(|o| &o.headings), pos);
            text_row.execute(params![
                id,
                "heading",
                x.level,
                pos as i64,
                x.text,
                None::<bool>,
                origin
            ])?;
        }
        let mut dir = self.conn.prepare_cached(
            "INSERT INTO rendered_directives (url_id, pos, directive, value, known, source, crawler, in_head,
             origin) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        let mut put_directive = |pos: usize, x: &Directive, origin: &str| {
            dir.execute(params![
                id,
                pos as i64,
                x.directive,
                x.value,
                x.known,
                x.source.as_str(),
                x.crawler,
                x.in_head,
                origin
            ])
        };
        for (pos, x) in d.directives.iter().enumerate() {
            put_directive(pos, x, origin_at(o.map(|o| &o.directives), pos))?;
        }
        let mut sd = self.conn.prepare_cached(
            "INSERT INTO rendered_structured_data (url_id, pos, kind, raw, parseable, origin)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for (pos, b) in d.structured.iter().enumerate() {
            let origin = origin_at(o.map(|o| &o.structured), pos);
            sd.execute(params![
                id,
                pos as i64,
                b.kind.as_str(),
                b.raw,
                b.parseable,
                origin
            ])?;
        }
        // What JavaScript removed: raw rows with origin `raw`, raw positions.
        if let (Some(raw), Some(o)) = (p.raw, o) {
            let raw_origin = Origin::Raw.as_str();
            for &i in &o.titles.raw_only {
                let t = &raw.titles[i];
                text_row.execute(params![
                    id, "title", None::<u8>, i as i64, t.text, t.in_head, raw_origin
                ])?;
            }
            for &i in &o.descriptions.raw_only {
                let t = &raw.meta_descriptions[i];
                text_row.execute(params![
                    id,
                    "meta_description",
                    None::<u8>,
                    i as i64,
                    t.text,
                    t.in_head,
                    raw_origin
                ])?;
            }
            for &i in &o.headings.raw_only {
                let x = &raw.headings[i];
                text_row.execute(params![
                    id,
                    "heading",
                    x.level,
                    i as i64,
                    x.text,
                    None::<bool>,
                    raw_origin
                ])?;
            }
            for &i in &o.directives.raw_only {
                put_directive(i, &raw.directives[i], raw_origin)?;
            }
            for &i in &o.structured.raw_only {
                let b = &raw.structured[i];
                sd.execute(params![
                    id,
                    i as i64,
                    b.kind.as_str(),
                    b.raw,
                    b.parseable,
                    raw_origin
                ])?;
            }
        }
        if let Some(c) = p.comparison {
            self.conn
                .prepare_cached(
                    "INSERT OR REPLACE INTO render_comparisons (url_id, words_raw, words_rendered, words_change,
                     links_raw, links_rendered, links_rendered_only, links_raw_only, title_diff,
                     description_diff, h1_diff, canonical_diff, noindex_diff, nofollow_diff, hreflang_diff,
                     structured_rendered_only, dep_text, dep_headings, dep_internal_links, dep_structured,
                     dep_images, raw_has_title, raw_has_description, raw_has_canonical, raw_has_directives,
                     dependence)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                     ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26)",
                )?
                .execute(params![
                    id,
                    c.words_raw,
                    c.words_rendered,
                    c.words_change,
                    c.links_raw,
                    c.links_rendered,
                    c.links_rendered_only,
                    c.links_raw_only,
                    c.title_diff.as_str(),
                    c.description_diff.as_str(),
                    c.h1_diff.as_str(),
                    c.canonical_diff.as_str(),
                    c.noindex_diff.as_str(),
                    c.nofollow_diff.as_str(),
                    c.hreflang_diff.as_str(),
                    c.structured_rendered_only,
                    c.dep_text,
                    c.dep_headings,
                    c.dep_internal_links,
                    c.dep_structured,
                    c.dep_images,
                    c.raw_has_title,
                    c.raw_has_description,
                    c.raw_has_canonical,
                    c.raw_has_directives,
                    c.dependence,
                ])?;
        }
        let mut gone = self
            .conn
            .prepare_cached("INSERT OR IGNORE INTO raw_only_links (from_id, pos) VALUES (?1, ?2)")?;
        for pos in p.raw_only_links {
            gone.execute(params![id, pos])?;
        }
        if let Some((packed, size)) = p.html {
            self.conn
                .prepare_cached(
                    "INSERT OR REPLACE INTO rendered_bodies (url_id, codec, size, body) VALUES (?1, ?2, ?3, ?4)",
                )?
                .execute(params![id, body::CODEC, size as i64, packed])?;
        }
        Ok(())
    }
}

fn unpack(codec: &str, size: i64, packed: &[u8]) -> Result<Vec<u8>> {
    if codec != body::CODEC {
        return Err(StoreError::Format(format!("unknown body codec {codec}")));
    }
    body::decompress(packed, size as usize)
        .map_err(|e| StoreError::Format(format!("stored HTML is damaged: {e}")))
}

/// The engine's version, recorded with every event.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// `macos-aarch64`, `windows-x86_64`, `linux-x86_64`.
pub fn os_name() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// A read-only connection. On read-only media SQLite cannot create the
/// shared-memory file a WAL database needs, so it retries with
/// `immutable=1` (the file cannot change under us there anyway).
pub fn connect_read_only(path: &Path) -> Result<Connection> {
    use rusqlite::OpenFlags;
    if !path.exists() {
        return Err(StoreError::Format(format!("{} does not exist", path.display())));
    }
    let flags =
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_URI;
    let probe =
        |c: &Connection| c.query_row("SELECT COUNT(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0));
    let first = Connection::open_with_flags(path, flags).and_then(|c| probe(&c).map(|_| c));
    match first {
        Ok(c) => {
            c.busy_timeout(std::time::Duration::from_secs(5))?;
            Ok(c)
        }
        Err(e) if is_corrupt(&e) => Err(damaged(path, e)),
        Err(_) => {
            let c = Connection::open_with_flags(immutable_uri(path), flags)?;
            probe(&c).map_err(|e| damaged(path, e))?;
            Ok(c)
        }
    }
}

/// A `file:` URI for `path` with `immutable=1`.
pub(crate) fn immutable_uri(path: &Path) -> String {
    let p = path.to_string_lossy().replace('\\', "/");
    let p = p.replace('%', "%25").replace('?', "%3f").replace('#', "%23");
    let p = if p.starts_with('/') { p } else { format!("/{p}") };
    format!("file:{p}?immutable=1")
}

pub(crate) fn is_corrupt(e: &rusqlite::Error) -> bool {
    matches!(
        e.sqlite_error_code(),
        Some(rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase)
    )
}

fn damaged(path: &Path, e: rusqlite::Error) -> StoreError {
    if is_corrupt(&e) {
        StoreError::Damaged(format!(
            "{} is damaged or is not a SpiderLens crawl file ({e})",
            path.display()
        ))
    } else {
        StoreError::Sql(e)
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

    fn extracted(html: &str) -> PageData {
        let url = url::Url::parse("https://a.test/a").unwrap();
        crate::extract::extract_page(&crate::extract::PageInput {
            url: &url,
            charset: None,
            body: html.as_bytes(),
            truncated: false,
            robots_token: "SpiderLens",
            keep_text: true,
        })
    }

    fn count(s: &CrawlStore, table: &str) -> i64 {
        s.conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    /// Every table of the rendering step (spec 014 data-model.md).
    pub(crate) const RENDER_TABLES: &[&str] = &[
        "renders",
        "render_inputs",
        "rendered_pages",
        "rendered_text",
        "rendered_directives",
        "rendered_structured_data",
        "rendered_links",
        "raw_only_links",
        "render_comparisons",
        "render_resources",
        "console_entries",
        "rendered_bodies",
        "screenshots",
    ];

    #[test]
    fn render_queue_moves_through_its_states() {
        let (_d, mut s) = store();
        let a = s.record(&disc("https://a.test/a", 0, QUEUE)).unwrap().id();
        let b = s.record(&disc("https://a.test/b", 1, QUEUE)).unwrap().id();
        let packed = body::compress(b"<p>raw a</p>").unwrap();
        s.enqueue_render(a, Some((&packed, 12))).unwrap();
        s.enqueue_render(b, None).unwrap();
        s.enqueue_render(a, None).unwrap();
        let q = s.renders_after(None, 10).unwrap();
        assert_eq!(
            q.iter().map(|e| e.url_id).collect::<Vec<_>>(),
            [a, b],
            "queued once, in order"
        );
        assert_eq!(s.renders_after(Some(q[0].seq), 10).unwrap().len(), 1);
        assert_eq!(s.render_input(a).unwrap().unwrap(), b"<p>raw a</p>");
        assert!(
            s.render_input(b).unwrap().is_none(),
            "no stored HTML and no input"
        );

        assert!(s.mark_rendering(a).unwrap());
        assert!(!s.mark_rendering(a).unwrap(), "only from queued");
        s.requeue_render(a).unwrap();
        assert_eq!(s.renders_after(None, 10).unwrap()[0].url_id, a, "keeps its place");
        assert!(s.mark_rendering(a).unwrap());
        assert_eq!(s.renders_after(None, 10).unwrap()[0].attempts, 0);

        // Stop, then resume: both come back, the input is kept.
        assert_eq!(s.stop_renders().unwrap(), 2);
        assert!(s.renders_after(None, 10).unwrap().is_empty());
        assert_eq!(count(&s, "render_inputs"), 1);
        assert_eq!(s.requeue_rendering().unwrap(), 2);
        let q = s.renders_after(None, 10).unwrap();
        assert_eq!(
            q.iter().map(|e| (e.url_id, e.attempts)).collect::<Vec<_>>(),
            [(a, 2), (b, 0)]
        );

        s.skip_render(b, "outside_rules", None).unwrap();
        let counts = s.render_counts().unwrap();
        assert!(counts.contains(&(RenderStatus::Queued, 1)));
        assert!(counts.contains(&(RenderStatus::NotRendered, 1)));
    }

    #[test]
    fn a_rendered_page_round_trips_through_every_table() {
        let (_d, mut s) = store();
        let a = s.record(&disc("https://a.test/a", 0, QUEUE)).unwrap().id();
        let input = body::compress(b"raw").unwrap();
        s.enqueue_render(a, Some((&input, 3))).unwrap();
        s.mark_rendering(a).unwrap();
        let raw = extracted(
            "<html><head><title>Loading</title><meta name=robots content=noarchive></head>\
             <body><h1>Old</h1></body></html>",
        );
        let rendered = extracted(
            r#"<html><head><title>Shop</title><meta name="description" content="Things">
            <script type="application/ld+json">{"@type":"Thing"}</script></head>
            <body><h1>Shop</h1><h2>More</h2><a href="/b">B</a></body></html>"#,
        );
        let internal = |u: &url::Url| u.host_str() == Some("a.test");
        let cmp = crate::extract::compare::compare(&raw, &rendered, "SpiderLens", &internal);
        let dom = "<!DOCTYPE html><html><body>rendered</body></html>";
        let packed = body::compress(dom.as_bytes()).unwrap();
        let link_ctx = rendered.refs[0].clone();
        s.add_rendered_link(
            &NewLink {
                from: a,
                to: None,
                raw: "/b",
                kind: LinkKind::Anchor,
                nofollow: false,
                page_nofollow: false,
                followed: true,
                pos: Some(7),
                target: Some("https://a.test/b"),
                skip_reason: None,
                context: Some(&link_ctx),
            },
            0,
        )
        .unwrap();
        let resources = [ResourceRow {
            url: "https://a.test/app.js",
            kind: "script",
            host: "https://a.test:443",
            third_party: false,
            outcome: "loaded",
            status: Some(200),
            error: None,
            robots_rule: None,
            bytes: 120,
        }];
        let console = [ConsoleRow {
            level: "error",
            message: "boom",
            source: Some(("https://a.test/app.js", 3, 9)),
        }];
        s.record_render(
            a,
            &RenderRecord {
                status: RenderStatus::Rendered,
                reason: None,
                reason_detail: None,
                render_ms: Some(840),
                window: Some((412, 915)),
                js_redirect: None,
                js_redirect_id: None,
                console_totals: (3, 1),
                resources_failed: 0,
                resources_blocked: 4,
                bot_challenge: false,
                rendered_at: 1_700_000_000_000,
                page: Some(RenderedPageRecord {
                    data: Some(&rendered),
                    extract_error: None,
                    raw: Some(&raw),
                    comparison: Some(&cmp),
                    noindex: false,
                    nofollow: false,
                    canonical: None,
                    canonical_count: 0,
                    canonical_conflict: false,
                    html: Some((&packed, dom.len())),
                    raw_only_links: &[2, 5],
                }),
                resources: &resources,
                console: &console,
                screenshot: Some((b"RIFFwebp", 412, 900)),
            },
        )
        .unwrap();

        let (state, ms, w, errors, blocked, seq): (String, i64, i64, i64, i64, Option<i64>) = s
            .conn
            .query_row(
                "SELECT state, render_ms, window_w, console_errors, resources_blocked, queue_seq
                 FROM renders WHERE url_id = ?1",
                [a],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .unwrap();
        assert_eq!(
            (state.as_str(), ms, w, errors, blocked, seq),
            ("rendered", 840, 412, 3, 4, None)
        );
        let (title, h1, stored): (String, String, bool) = s
            .conn
            .query_row(
                "SELECT title, h1_1, rendered_html_stored FROM rendered_pages WHERE url_id = ?1",
                [a],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((title.as_str(), h1.as_str(), stored), ("Shop", "Shop", true));
        let texts: Vec<(String, String, String)> = s
            .conn
            .prepare(
                "SELECT kind, text, origin FROM rendered_text WHERE url_id = ?1 ORDER BY origin, kind, pos",
            )
            .unwrap()
            .query_map([a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        let has = |k: &str, t: &str, o: &str| texts.iter().any(|x| x.0 == k && x.1 == t && x.2 == o);
        assert!(has("title", "Shop", "rendered"), "{texts:?}");
        assert!(
            has("title", "Loading", "raw"),
            "what JavaScript removed: {texts:?}"
        );
        assert!(has("heading", "Old", "raw"));
        assert!(has("meta_description", "Things", "rendered"));
        let dir: (String, String) = s
            .conn
            .query_row(
                "SELECT directive, origin FROM rendered_directives WHERE url_id = ?1",
                [a],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(dir, ("noarchive".into(), "raw".into()));
        let sd: String = s
            .conn
            .query_row(
                "SELECT origin FROM rendered_structured_data WHERE url_id = ?1",
                [a],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(sd, "rendered");
        let (title_diff, dep): (String, f64) = s
            .conn
            .query_row(
                "SELECT title_diff, dependence FROM render_comparisons WHERE url_id = ?1",
                [a],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(title_diff, "changed");
        assert!(dep > 0.0);
        let (raw_link, rpos, text): (String, i64, String) = s
            .conn
            .query_row(
                "SELECT raw, rpos, text FROM rendered_links WHERE from_id = ?1",
                [a],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((raw_link.as_str(), rpos, text.as_str()), ("/b", 0, "B"));
        assert_eq!(count(&s, "raw_only_links"), 2);
        let res: (String, String, i64) = s
            .conn
            .query_row(
                "SELECT type, outcome, bytes FROM render_resources WHERE url_id = ?1",
                [a],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(res, ("script".into(), "loaded".into(), 120));
        let con: (String, String, i64) = s
            .conn
            .query_row(
                "SELECT level, message, line FROM console_entries WHERE url_id = ?1",
                [a],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(con, ("error".into(), "boom".into(), 3));
        assert_eq!(s.rendered_html(a).unwrap().unwrap(), dom.as_bytes());
        assert_eq!(
            s.screenshot(a).unwrap().unwrap(),
            (b"RIFFwebp".to_vec(), 412, 900)
        );
        assert_eq!(
            count(&s, "render_inputs"),
            0,
            "the input goes when the render is final"
        );

        s.set_renderer(r#"{"name":"Chromium","version":"141","source":"installed"}"#)
            .unwrap();
        assert_eq!(s.meta("render_version").unwrap().as_deref(), Some(RENDER_VERSION));
        s.set_meta("data_features", "extraction,timing").unwrap();
        s.add_data_features(&["rendering", "screenshots", "rendering"])
            .unwrap();
        assert_eq!(
            s.meta("data_features").unwrap().as_deref(),
            Some("extraction,timing,rendering,screenshots")
        );
    }

    #[test]
    fn a_failed_render_writes_only_its_state_and_stores_at_most_the_limits() {
        let (_d, mut s) = store();
        let a = s.record(&disc("https://a.test/a", 0, QUEUE)).unwrap().id();
        s.enqueue_render(a, None).unwrap();
        let urls: Vec<String> = (0..600).map(|i| format!("https://a.test/{i}.js")).collect();
        let resources: Vec<ResourceRow> = urls
            .iter()
            .map(|u| ResourceRow {
                url: u,
                kind: "script",
                host: "https://a.test:443",
                third_party: false,
                outcome: "failed",
                status: None,
                error: Some("timeout"),
                robots_rule: None,
                bytes: 0,
            })
            .collect();
        let console: Vec<ConsoleRow> = (0..60)
            .map(|_| ConsoleRow {
                level: "warning",
                message: "w",
                source: None,
            })
            .collect();
        s.record_render(
            a,
            &RenderRecord {
                status: RenderStatus::Failed,
                reason: Some("crash"),
                reason_detail: Some("renderer crashed"),
                render_ms: None,
                window: None,
                js_redirect: None,
                js_redirect_id: None,
                console_totals: (0, 60),
                resources_failed: 600,
                resources_blocked: 0,
                bot_challenge: false,
                rendered_at: 1,
                page: None,
                resources: &resources,
                console: &console,
                screenshot: None,
            },
        )
        .unwrap();
        assert_eq!(count(&s, "render_resources"), MAX_RESOURCE_ROWS as i64);
        assert_eq!(count(&s, "console_entries"), MAX_CONSOLE_ROWS as i64);
        assert_eq!(count(&s, "rendered_pages"), 0);
        assert_eq!(count(&s, "render_comparisons"), 0);
        let (reason, failed): (String, i64) = s
            .conn
            .query_row(
                "SELECT reason, resources_failed FROM renders WHERE url_id = ?1",
                [a],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((reason.as_str(), failed), ("crash", 600));
    }

    #[test]
    fn a_format_2_file_has_no_render_tables_and_reads_as_empty() {
        let (_d, s) = store();
        for t in RENDER_TABLES {
            s.conn.execute_batch(&format!("DROP TABLE {t}")).unwrap();
        }
        assert_eq!(s.stop_renders().unwrap(), 0);
        assert_eq!(s.requeue_rendering().unwrap(), 0);
        assert!(s.render_counts().unwrap().is_empty());
        assert!(s.rendered_html(1).unwrap().is_none());
        assert!(s.screenshot(1).unwrap().is_none());
    }
}
