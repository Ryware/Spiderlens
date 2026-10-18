//! One read-only connection to a crawl file, owned by one thread.
//!
//! The crawl keeps writing through its own connection; in WAL mode readers
//! never block it and always see a consistent committed state.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use rusqlite::functions::FunctionFlags;
use rusqlite::{Connection, OptionalExtension};

use crate::features::Features;
use crate::issues::ResolvedSettings;
use crate::sql::Thresholds;
use crate::tabs::{Derived, TabDef};
use crate::values::{self, Values};
use crate::view::View;
use crate::{Error, Result, sql};

pub struct Reader {
    pub(crate) conn: Connection,
    pub(crate) path: PathBuf,
    pub(crate) features: Features,
    pub(crate) views: HashMap<u32, View>,
    pub(crate) next_view: u32,
    /// Crawl data generation each derived table was built from. A settings
    /// change removes the issue hits itself; the other tables stay.
    pub(crate) derived: HashMap<Derived, u64>,
    /// The values the connection's temporary views show now.
    values: Values,
    /// Issue settings in force, and the values `thr()` reads (spec 009).
    pub(crate) issue_settings: ResolvedSettings,
    pub(crate) thresholds: Arc<RwLock<Thresholds>>,
    /// See [`Reader::set_partial_sink`].
    pub(crate) partial: Option<Box<dyn FnMut(crate::issues::IssuesSummary) + Send>>,
    /// The file's custom tabs and the data version they were built at.
    custom_tabs: Option<(u64, Vec<TabDef>)>,
    /// The sidebar counts of the last data version asked for.
    pub(crate) tab_counts: Option<crate::counts::TabCounts>,
    version: u64,
    /// Counts crawl data changes only (not settings changes).
    pub(crate) data_gen: u64,
    data_version: i64,
    /// Segment membership (spec 027 part B).
    pub(crate) segments: crate::segments::State,
}

/// Opens a crawl file the way every reader in this crate does: read-only,
/// with the SQL functions registered. `thr()` reads `settings`' thresholds.
pub(crate) fn connect(
    path: &Path,
    settings: &ResolvedSettings,
) -> Result<(Connection, Features, Arc<RwLock<Thresholds>>)> {
    // Read-only, with an `immutable=1` retry for read-only media (spec 006
    // FR-021); a damaged file is reported as such (FR-038).
    let conn = spiderlens_engine::crawl::store::connect_read_only(path)
        .map_err(|e| Error::invalid(e.to_string()))?;
    setup(conn, path, settings)
}

/// Opens a crawl file read-only as `main` on a connection that can still
/// write to databases it attaches (spec 012 plan D1): the crawl is opened
/// through a `mode=ro` URI, so SQLite refuses any write to it, while a
/// scratch file attached later stays writable.
pub fn connect_attachable(path: &Path) -> Result<(Connection, Features)> {
    connect_attachable_with(path, &ResolvedSettings::default())
}

/// [`connect_attachable`] with `thr()` reading `settings`' thresholds, so a
/// filter and an issue rule that share a threshold agree (spec 012 FR-023).
pub fn connect_attachable_with(path: &Path, settings: &ResolvedSettings) -> Result<(Connection, Features)> {
    if !path.exists() {
        return Err(Error::invalid(format!("{} does not exist", path.display())));
    }
    let conn = Connection::open_with_flags(
        read_only_uri(path),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
            | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    let (conn, features, _) = setup(conn, path, settings)?;
    Ok((conn, features))
}

/// `file:<path>?mode=ro`, percent-encoding what SQLite's URI parser would
/// otherwise read as syntax, and every non-ASCII byte.
fn read_only_uri(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file:");
    if raw.as_bytes().get(1) == Some(&b':') {
        // A Windows drive letter: file:/C:/...
        out.push('/');
    }
    for b in raw.bytes() {
        if b.is_ascii() && !matches!(b, b'%' | b'?' | b'#') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out.push_str("?mode=ro");
    out
}

fn setup(
    conn: Connection,
    path: &Path,
    settings: &ResolvedSettings,
) -> Result<(Connection, Features, Arc<RwLock<Thresholds>>)> {
    let format: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = 'format'", [], |r| r.get(0))
        .optional()
        .unwrap_or(None);
    if format.as_deref() != Some(spiderlens_engine::crawl::store::FORMAT) {
        return Err(Error::invalid(format!(
            "{} is not a SpiderLens crawl file",
            path.display()
        )));
    }
    // A bounded page cache: memory stays the same whatever the crawl size.
    conn.pragma_update(None, "cache_size", -32_768)?;
    conn.pragma_update(None, "temp_store", "FILE")?;
    sql::register(&conn)?;
    crate::segments::create_tables(&conn)?;
    let thresholds = Arc::new(RwLock::new(settings.thresholds()));
    sql::register_thresholds(&conn, thresholds.clone())?;
    let features = Features::detect(&conn)?;
    stub_render_tables(&conn)?;
    sql::register_a11y(&conn, &features.a11y)?;
    let limit = features.redirect_limit;
    conn.create_scalar_function(
        "redirect_limit",
        0,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        move |_| Ok(limit),
    )?;
    // The JavaScript tab's thresholds, as the crawl was configured (spec 014).
    let render = features.render_thresholds;
    for (name, value) in [
        ("js_content_percent", render.js_content_percent),
        ("js_content_words", render.js_content_words),
        ("hidden_content_percent", render.hidden_percent),
    ] {
        conn.create_scalar_function(
            name,
            0,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            move |_| Ok(value),
        )?;
    }
    let token = features.robots_token.clone();
    conn.create_scalar_function(
        "robots_token",
        0,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        move |_| Ok(token.clone()),
    )?;
    Ok((conn, features, thresholds))
}

/// A crawl file made before rendering (migrate step 16) has no `renders`
/// or `render_comparisons` table. The JavaScript tab's row condition reads
/// `renders` for every file, so a file without it gets empty `temp` tables
/// of the same name: the tab lists nothing and the file keeps opening (spec
/// 014 FR-034). Temporary tables work on the read-only connection.
fn stub_render_tables(conn: &Connection) -> rusqlite::Result<()> {
    let has: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM main.sqlite_master WHERE type = 'table' AND name = 'renders')",
        [],
        |r| r.get(0),
    )?;
    if has {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS renders (
            url_id INTEGER PRIMARY KEY, state TEXT NOT NULL, reason TEXT, reason_detail TEXT,
            queue_seq INTEGER, attempts INTEGER NOT NULL DEFAULT 0, render_ms INTEGER,
            window_w INTEGER, window_h INTEGER, js_redirect TEXT, js_redirect_id INTEGER,
            console_errors INTEGER, console_warnings INTEGER, resources_failed INTEGER,
            resources_blocked INTEGER, bot_challenge INTEGER, rendered_at INTEGER);
         CREATE TEMP TABLE IF NOT EXISTS render_comparisons (
            url_id INTEGER PRIMARY KEY, words_raw INTEGER, words_rendered INTEGER, words_change INTEGER,
            links_raw INTEGER, links_rendered INTEGER, links_rendered_only INTEGER, links_raw_only INTEGER,
            title_diff TEXT, description_diff TEXT, h1_diff TEXT, canonical_diff TEXT, noindex_diff TEXT,
            nofollow_diff TEXT, hreflang_diff TEXT, structured_rendered_only INTEGER,
            dep_text REAL, dep_headings REAL, dep_internal_links REAL, dep_structured REAL,
            dep_images REAL, raw_has_title INTEGER, raw_has_description INTEGER,
            raw_has_canonical INTEGER, raw_has_directives INTEGER, dependence REAL);",
    )
}

/// Opens a crawl file to record results in it (issue settings, analysis
/// outputs), waiting up to 5 s for a writer.
pub(crate) fn connect_writable(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    let format: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = 'format'", [], |r| r.get(0))
        .optional()?;
    if format.as_deref() != Some(spiderlens_engine::crawl::store::FORMAT) {
        return Err(Error::invalid("not a crawl file this version writes"));
    }
    Ok(conn)
}

impl Reader {
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_with(path, ResolvedSettings::default())
    }

    /// Opens with issue settings already applied (spec 009, the active
    /// profile's or the file's own).
    pub fn open_with(path: &Path, issue_settings: ResolvedSettings) -> Result<Self> {
        let (conn, features, thresholds) = connect(path, &issue_settings)?;
        let data_version = conn.query_row("PRAGMA data_version", [], |r| r.get(0))?;
        let stored = crate::segments::stored(&conn);
        let mut reader = Self {
            conn,
            path: path.to_path_buf(),
            features,
            views: HashMap::new(),
            next_view: 1,
            derived: HashMap::new(),
            values: Values::Raw,
            issue_settings,
            thresholds,
            partial: None,
            custom_tabs: None,
            tab_counts: None,
            version: 1,
            data_gen: 1,
            data_version,
            segments: crate::segments::State::new(Vec::new()),
        };
        if !stored.is_empty() {
            reader.set_segments(stored)?;
        }
        Ok(reader)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The reader's connection, for queries that live outside the views
    /// (near duplicates). Its `temp` schema holds analysis results of files
    /// that cannot be written.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn features(&self) -> &Features {
        &self.features
    }

    /// Reads the file's data list again: analyses that run after the crawl
    /// (spec 015's AI checks) add data to a file that is already open.
    pub fn refresh_features(&mut self) -> Result<&Features> {
        self.features = Features::detect(&self.conn)?;
        Ok(&self.features)
    }

    /// Changes whenever the crawl commits new data (cheap: no table read).
    pub fn version(&mut self) -> Result<u64> {
        let dv: i64 = self.conn.query_row("PRAGMA data_version", [], |r| r.get(0))?;
        if dv != self.data_version {
            self.data_version = dv;
            self.version += 1;
            self.data_gen += 1;
        }
        Ok(self.version)
    }

    /// Switches the connection to raw or rendered values (spec 014). The
    /// derived tables were built from the other values, so they are built
    /// again when next needed.
    pub(crate) fn use_values(&mut self, values: Values) -> Result<()> {
        if values != self.values {
            values::apply(&self.conn, values)?;
            self.values = values;
            self.derived.clear();
        }
        Ok(())
    }

    /// Marks everything shown as changed without new crawl data (a settings
    /// change), so views and counts refresh.
    pub(crate) fn bump_version(&mut self) {
        self.version += 1;
    }

    /// This file's custom tabs (spec 013), rebuilt when the data changed:
    /// a rerun can add or rename rules.
    pub(crate) fn custom_tabs(&mut self) -> Result<&[TabDef]> {
        let v = self.version()?;
        if self.custom_tabs.as_ref().is_none_or(|(at, _)| *at != v) {
            self.custom_tabs = Some((v, crate::custom::tabs(&self.conn)?));
        }
        Ok(&self.custom_tabs.as_ref().expect("just built").1)
    }

    /// Every tab this file shows: the fixed ones, then its custom tabs.
    pub fn tabs(&mut self) -> Result<Vec<TabDef>> {
        let mut all = crate::tabs::tabs().to_vec();
        // One tab per issue rule too (spec 009 D10): `issue:<rule id>`.
        all.extend(crate::tabs::issue_tabs().iter().cloned());
        all.extend(self.custom_tabs()?.iter().cloned());
        Ok(all)
    }

    /// A tab of this file by id.
    pub fn find_tab(&mut self, id: &str) -> Result<Cow<'static, TabDef>> {
        let custom = self.custom_tabs()?;
        crate::tabs::find(id, custom).ok_or_else(|| Error::invalid(format!("unknown tab {id:?}")))
    }

    /// Makes sure each derived table is current for this crawl data,
    /// updating it in place when it can (see `tabs::update_derived`).
    pub(crate) fn ensure_derived(&mut self, needed: impl IntoIterator<Item = Derived>) -> Result<()> {
        self.ensure_segments()?;
        self.version()?;
        let g = self.data_gen;
        for d in needed {
            // Weighted Link Score reads the link positions (spec 023 FR-025).
            if d == Derived::LinkMetrics && self.features.link_settings.weighting.is_some() {
                self.ensure_derived([Derived::LinkPositions])?;
            }
            if self.derived.get(&d) != Some(&g) {
                if matches!(d, Derived::LinkPositions | Derived::LinkMetrics) && self.use_stored_links()? {
                    self.derived.insert(d, g);
                    continue;
                }
                if d == Derived::IssueHits {
                    self.build_issue_hits()?;
                } else {
                    // Forgotten first: a build that fails or is interrupted
                    // half way is rebuilt from scratch next time.
                    let built = self.derived.remove(&d).is_some();
                    if !(built && crate::tabs::update_derived(&self.conn, d, &self.features)?) {
                        crate::tabs::build_derived(&self.conn, d, &self.features)?;
                    }
                }
                self.derived.insert(d, g);
            }
        }
        Ok(())
    }

    /// When the file holds current Link Score results for the settings shown
    /// (spec 023 plan D5), points the unqualified `link_positions` and
    /// `link_metrics` at them and builds nothing. Any temp copy from an
    /// earlier build is dropped so it cannot shadow the file's.
    fn use_stored_links(&self) -> Result<bool> {
        if crate::analysis::stored_run(&self.conn, &self.features.link_settings)?.is_none() {
            return Ok(false);
        }
        crate::analysis::drop_temp(&self.conn, "link_positions")?;
        crate::analysis::drop_temp(&self.conn, "link_metrics")?;
        self.conn
            .execute_batch("CREATE TEMP VIEW link_metrics AS SELECT * FROM main.link_score;")?;
        Ok(true)
    }

    /// Records that a derived table was just built for the current data.
    pub(crate) fn mark_built(&mut self, d: Derived) -> Result<()> {
        self.version()?;
        self.derived.insert(d, self.data_gen);
        Ok(())
    }

    /// Whether a derived table is current, without building it.
    pub(crate) fn has_derived(&mut self, d: Derived) -> Result<bool> {
        self.version()?;
        Ok(self.derived.get(&d) == Some(&self.data_gen))
    }

    /// Counts accessibility results against another WCAG target without
    /// checking again (spec 026 FR-020).
    pub fn set_a11y_target(&mut self, wcag: spiderlens_config::WcagTarget) -> Result<()> {
        self.features.a11y.wcag = wcag;
        sql::register_a11y(&self.conn, &self.features.a11y)?;
        // Rules that read the target are evaluated again.
        self.derived.remove(&crate::tabs::Derived::IssueHits);
        self.bump_version();
        Ok(())
    }

    pub fn link_settings(&self) -> &spiderlens_config::LinkAnalysisSettings {
        &self.features.link_settings
    }

    /// Replaces the link analysis settings (Settings › Link analysis). The
    /// positions and Link Score are worked out again on next use (FR-023).
    pub fn set_link_settings(&mut self, settings: spiderlens_config::LinkAnalysisSettings) {
        if settings != self.features.link_settings {
            self.features.link_settings = settings;
            self.derived.remove(&Derived::LinkPositions);
            self.derived.remove(&Derived::LinkMetrics);
            // Open views read the new values on their next refresh.
            self.version += 1;
        }
    }

    /// A handle that stops this reader's running query from another thread
    /// (a superseded sort or search).
    pub fn interrupt_handle(&self) -> rusqlite::InterruptHandle {
        self.conn.get_interrupt_handle()
    }
}
