//! One read-only connection to a crawl file, owned by one thread.
//!
//! The crawl keeps writing through its own connection; in WAL mode readers
//! never block it and always see a consistent committed state.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use rusqlite::functions::FunctionFlags;
use rusqlite::{Connection, OpenFlags, OptionalExtension};

use crate::features::Features;
use crate::issues::ResolvedSettings;
use crate::sql::Thresholds;
use crate::tabs::Derived;
use crate::view::View;
use crate::{Error, Result, sql};

pub struct Reader {
    pub(crate) conn: Connection,
    pub(crate) path: PathBuf,
    pub(crate) features: Features,
    pub(crate) views: HashMap<u32, View>,
    pub(crate) next_view: u32,
    /// Data version each derived table was built from.
    pub(crate) derived: HashMap<Derived, u64>,
    /// Issue settings in force, and the values `thr()` reads (spec 009).
    pub(crate) issue_settings: ResolvedSettings,
    pub(crate) thresholds: Arc<RwLock<Thresholds>>,
    version: u64,
    data_version: i64,
}

/// Opens a crawl file the way every reader in this crate does: read-only,
/// with the SQL functions registered. `thr()` reads `settings`' thresholds.
pub(crate) fn connect(
    path: &Path,
    settings: &ResolvedSettings,
) -> Result<(Connection, Features, Arc<RwLock<Thresholds>>)> {
    if !path.exists() {
        return Err(Error::invalid(format!("{} does not exist", path.display())));
    }
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_URI,
    )?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
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
    let thresholds = Arc::new(RwLock::new(settings.thresholds()));
    sql::register_thresholds(&conn, thresholds.clone())?;
    let features = Features::detect(&conn)?;
    let limit = features.redirect_limit;
    conn.create_scalar_function(
        "redirect_limit",
        0,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        move |_| Ok(limit),
    )?;
    let token = features.robots_token.clone();
    conn.create_scalar_function(
        "robots_token",
        0,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        move |_| Ok(token.clone()),
    )?;
    Ok((conn, features, thresholds))
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
        Ok(Self {
            conn,
            path: path.to_path_buf(),
            features,
            views: HashMap::new(),
            next_view: 1,
            derived: HashMap::new(),
            issue_settings,
            thresholds,
            version: 1,
            data_version,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn features(&self) -> &Features {
        &self.features
    }

    /// Changes whenever the crawl commits new data (cheap: no table read).
    pub fn version(&mut self) -> Result<u64> {
        let dv: i64 = self.conn.query_row("PRAGMA data_version", [], |r| r.get(0))?;
        if dv != self.data_version {
            self.data_version = dv;
            self.version += 1;
        }
        Ok(self.version)
    }

    /// Marks everything shown as changed without new crawl data (a settings
    /// change), so views and counts refresh.
    pub(crate) fn bump_version(&mut self) {
        self.version += 1;
    }

    /// Makes sure each derived table is current for this data version.
    pub(crate) fn ensure_derived(&mut self, needed: impl IntoIterator<Item = Derived>) -> Result<()> {
        let v = self.version()?;
        for d in needed {
            if self.derived.get(&d) != Some(&v) {
                if d == Derived::IssueHits {
                    self.build_issue_hits()?;
                } else {
                    crate::tabs::build_derived(&self.conn, d)?;
                }
                self.derived.insert(d, self.version);
            }
        }
        Ok(())
    }

    /// A handle that stops this reader's running query from another thread
    /// (a superseded sort or search).
    pub fn interrupt_handle(&self) -> rusqlite::InterruptHandle {
        self.conn.get_interrupt_handle()
    }
}
