//! SpiderLens crawl comparison (spec 012): what changed between a previous
//! and a current crawl file.
//!
//! The comparison is a workspace: a temporary SQLite file holding one row
//! per URL per crawl and the match between them (plan D1, D2). Each crawl
//! file is opened read-only as `main` of its own connection, exactly as the
//! results table reads it, so every tab, filter and column definition runs
//! unchanged; the workspace is attached as `cmp` and filled with one
//! `INSERT … SELECT` per batch. After both sides are in, every count, list
//! and difference is a query over the workspace alone. Neither crawl file is
//! ever written: SQLite refuses writes to a `mode=ro` database.

mod elements;
mod groups;
mod info;
mod issues;
mod registry;
mod report;
mod side;
mod sqlfns;
mod summary;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::Connection;
use serde::Serialize;
use spiderlens_config::{CompareSettings, CompareSettingsError, IssueSettings, MapRule};
use spiderlens_results::Reader;
use ts_rs::TS;

pub use elements::ElementKind;
pub use groups::{FilterChange, Group, GroupColumn, GroupPage, GroupRow, GroupSort, UrlGroup, Which};
pub use info::{ConfigDiff, CrawlInfo};
pub use issues::{IssueBasis, IssueChange, IssueDelta};
pub use report::{ComparisonReport, ElementReport};
pub use summary::{ElementDelta, FilterDelta, StateDelta, Summary};

use elements::Element;
use registry::Layout;
use sqlfns::Mapping;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("crawl or workspace file error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Results(#[from] spiderlens_results::Error),
    #[error(transparent)]
    Settings(#[from] CompareSettingsError),
    #[error("could not write the file: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("comparison cancelled")]
    Cancelled,
}

impl Error {
    pub(crate) fn invalid(msg: impl Into<String>) -> Self {
        Self::Invalid(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Where a running comparison is (contracts: `compare-progress`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Phase {
    Open,
    Previous,
    Current,
    Issues,
    Match,
    Counts,
    Done,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Progress {
    pub phase: Phase,
    #[ts(type = "number")]
    pub done: u64,
    #[ts(type = "number")]
    pub total: u64,
}

/// What the setup sheet shows before a comparison runs (FR-003, FR-007).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Setup {
    pub previous: CrawlInfo,
    pub current: CrawlInfo,
    pub config_diffs: Vec<ConfigDiff>,
    /// A host rule when the two crawls start on different origins.
    pub suggested_mapping: Option<MapRule>,
}

/// One URL through the mapping tester (FR-008).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct MappingTest {
    pub url: String,
    pub mapped: String,
    pub exists_in_current: bool,
    /// The current crawl's final state for the mapped URL.
    pub state: Option<String>,
}

/// Both crawls' details for one matched URL (FR-021).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PairDetail {
    pub previous: Option<spiderlens_results::UrlDetail>,
    pub current: Option<spiderlens_results::UrlDetail>,
}

/// One side of the comparison, kept open read-only for row values and
/// details.
pub(crate) struct Side {
    pub path: PathBuf,
    pub info: CrawlInfo,
    pub features: spiderlens_results::Features,
    pub conn: Connection,
    pub reader: Option<Reader>,
    /// Derived tables already built on `conn`.
    pub derived: Vec<spiderlens_results::Derived>,
}

/// A finished comparison: the workspace and both crawl files, read only.
pub struct Comparison {
    ws: Connection,
    ws_file: tempfile::TempPath,
    pub(crate) sides: [Side; 2],
    pub(crate) layout: Layout,
    pub(crate) elements: Vec<Element>,
    pub(crate) settings: CompareSettings,
    pub(crate) summary: Summary,
    pub(crate) views: HashMap<u32, groups::GroupView>,
    pub(crate) next_view: u32,
}

/// Opens both files and describes them; nothing is compared yet.
pub fn setup(previous: &Path, current: &Path) -> Result<Setup> {
    check_distinct(previous, current)?;
    let (pc, _) = spiderlens_results::connect_attachable(previous)?;
    let (cc, _) = spiderlens_results::connect_attachable(current)?;
    let previous = CrawlInfo::read(&pc, previous)?;
    let current = CrawlInfo::read(&cc, current)?;
    let suggested_mapping = match (previous.origin(), current.origin()) {
        (Some(a), Some(b)) if a != b => Some(MapRule {
            find: format!("{a}/"),
            replace: format!("{b}/"),
            regex: false,
        }),
        _ => None,
    };
    Ok(Setup {
        config_diffs: info::config_diffs(&previous, &current),
        previous,
        current,
        suggested_mapping,
    })
}

/// Runs `rules` on sample URLs, or on the first `from_previous` URLs of the
/// previous crawl, and looks each result up in the current crawl (FR-008).
pub fn test_mapping(
    previous: &Path,
    current: &Path,
    rules: &[MapRule],
    samples: &[String],
    from_previous: u32,
) -> Result<Vec<MappingTest>> {
    let mapping = Mapping::new(rules)?;
    let mut urls: Vec<String> = samples
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if from_previous > 0 {
        let (pc, _) = spiderlens_results::connect_attachable(previous)?;
        let mut stmt = pc.prepare("SELECT url FROM urls WHERE internal = 1 ORDER BY id LIMIT ?1")?;
        let rows = stmt.query_map([from_previous], |r| r.get::<_, String>(0))?;
        for r in rows {
            urls.push(r?);
        }
    }
    let (cc, _) = spiderlens_results::connect_attachable(current)?;
    let mut stmt = cc.prepare(&format!(
        "SELECT {} FROM urls u WHERE u.url = ?1",
        spiderlens_results::columns::FINAL_STATE.expr
    ))?;
    urls.into_iter()
        .map(|url| {
            let mapped = mapping.apply(&url);
            let state: Option<String> =
                stmt.query_row([&mapped], |r| r.get(0))
                    .map(Some)
                    .or_else(|e| match e {
                        rusqlite::Error::QueryReturnedNoRows => Ok(None),
                        e => Err(e),
                    })?;
            Ok(MappingTest {
                exists_in_current: state.is_some(),
                url,
                mapped,
                state,
            })
        })
        .collect()
}

fn check_distinct(previous: &Path, current: &Path) -> Result<()> {
    let same = match (previous.canonicalize(), current.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => previous == current,
    };
    if same {
        return Err(Error::invalid(
            "Choose two different crawl files: a crawl compared with itself has no changes.",
        ));
    }
    Ok(())
}

impl Comparison {
    /// Compares `previous` with `current`. `app_issues` are the app's issue
    /// settings, used when `settings.issue_settings` asks for them. `cancel`
    /// is checked between batches; a cancelled run leaves no workspace
    /// behind.
    pub fn run(
        previous: &Path,
        current: &Path,
        settings: CompareSettings,
        app_issues: Option<&IssueSettings>,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<Self> {
        settings.validate()?;
        check_distinct(previous, current)?;
        let mapping = Mapping::new(&settings.mapping)?;
        let elements = elements::resolve(&settings.elements)?;
        let layout = Layout::new();
        let issue_settings =
            issues::resolve_settings(previous, current, settings.issue_settings, app_issues)?;
        progress(Progress {
            phase: Phase::Open,
            done: 0,
            total: 1,
        });

        let ws_file = tempfile::Builder::new()
            .prefix("spiderlens-compare-")
            .suffix(".sqlite")
            .tempfile()?
            .into_temp_path();
        let ws = Connection::open(&ws_file)?;
        ws.execute_batch("PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF; PRAGMA temp_store = FILE;")?;
        ws.pragma_update(None, "cache_size", -32_768)?;
        sqlfns::register_common(&ws)?;
        side::create_tables(&ws, &layout, &elements)?;

        let mut sides = Vec::with_capacity(2);
        for (n, path) in [previous, current].into_iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            let phase = if n == 0 { Phase::Previous } else { Phase::Current };
            let (conn, features) = spiderlens_results::connect_attachable_with(path, &issue_settings)?;
            sqlfns::register_common(&conn)?;
            if n == 0 {
                sqlfns::register_mapping(&conn, mapping.clone())?;
            }
            let info = CrawlInfo::read(&conn, path)?;
            let derived = side::fill(
                &conn,
                &ws_file,
                n as u8,
                &features,
                &layout,
                &elements,
                n == 0 && !mapping.is_empty(),
                cancel,
                &mut |done, total| progress(Progress { phase, done, total }),
            )?;
            sides.push(Side {
                path: path.to_path_buf(),
                info,
                features,
                conn,
                reader: None,
                derived,
            });
        }
        let sides: [Side; 2] = sides.try_into().map_err(|_| Error::invalid("two sides"))?;

        // Both crawls are evaluated into their temp schemas and the hits
        // copied to the workspace (FR-024, FR-024a).
        for (n, side) in sides.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            progress(Progress {
                phase: Phase::Issues,
                done: n as u64,
                total: 2,
            });
            issues::collect(side, &ws_file, n as u8, &issue_settings)?;
        }

        progress(Progress {
            phase: Phase::Match,
            done: 0,
            total: 1,
        });
        summary::build_matches(&ws, sides[1].info.complete)?;
        progress(Progress {
            phase: Phase::Counts,
            done: 0,
            total: 1,
        });
        let issue_basis = issues::basis(settings.issue_settings, &issue_settings);
        let summary = summary::summary(&ws, &sides, &layout, &elements, &issue_settings, issue_basis)?;
        progress(Progress {
            phase: Phase::Done,
            done: 1,
            total: 1,
        });
        Ok(Self {
            ws,
            ws_file,
            sides,
            layout,
            elements,
            settings,
            summary,
            views: HashMap::new(),
            next_view: 1,
        })
    }

    pub fn settings(&self) -> &CompareSettings {
        &self.settings
    }

    pub fn summary(&self) -> &Summary {
        &self.summary
    }

    pub fn workspace_path(&self) -> &Path {
        &self.ws_file
    }

    /// Changes tracked elements and thresholds and recounts, without
    /// reading the crawl files again (FR-019). Turning on an element that
    /// was off needs nothing more either: every element is stored.
    pub fn set_elements(
        &mut self,
        overrides: Vec<spiderlens_config::ElementSetting>,
    ) -> Result<Vec<ElementDelta>> {
        self.elements = elements::resolve(&overrides)?;
        self.settings.elements = overrides;
        self.summary.elements = summary::element_deltas(&self.ws, &self.sides, &self.elements)?;
        Ok(self.summary.elements.clone())
    }

    /// Both crawls' details for a pair of URL ids (FR-021).
    pub fn detail(&mut self, prev_id: Option<i64>, cur_id: Option<i64>) -> Result<PairDetail> {
        let mut get = |n: usize, id: Option<i64>| -> Result<Option<spiderlens_results::UrlDetail>> {
            let Some(id) = id else { return Ok(None) };
            let side = &mut self.sides[n];
            if side.reader.is_none() {
                side.reader = Some(Reader::open(&side.path)?);
            }
            Ok(Some(side.reader.as_mut().expect("opened").detail(id)?))
        };
        Ok(PairDetail {
            previous: get(0, prev_id)?,
            current: get(1, cur_id)?,
        })
    }
}

/// The crawl to compare `current` with by default (FR-001): of the
/// `candidates` (recent crawls, unsaved crawls), the most recent other file,
/// no newer than `current`, that starts on the same origin. Files that cannot
/// be read are skipped.
pub fn suggest_previous(candidates: impl IntoIterator<Item = PathBuf>, current: &Path) -> Option<PathBuf> {
    let origin = |p: &Path| -> Option<String> {
        let (conn, _) = spiderlens_results::connect_attachable(p).ok()?;
        CrawlInfo::read(&conn, p).ok()?.origin()
    };
    let mtime = |p: &Path| p.metadata().and_then(|m| m.modified()).ok();
    let want = origin(current)?;
    let limit = mtime(current);
    let current = current.canonicalize().unwrap_or_else(|_| current.to_path_buf());
    let mut seen = std::collections::HashSet::new();
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = candidates
        .into_iter()
        .filter(|p| p.extension().is_some_and(|x| x == "spiderlens"))
        .filter(|p| {
            let c = p.canonicalize().unwrap_or_else(|_| p.clone());
            c != current && seen.insert(c)
        })
        .filter_map(|p| Some((mtime(&p)?, p)))
        .filter(|(t, _)| limit.is_none_or(|l| *t <= l))
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    files
        .into_iter()
        .map(|(_, p)| p)
        .find(|p| origin(p).as_deref() == Some(want.as_str()))
}

/// The crawl files directly inside `dir`.
pub fn crawl_files(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default()
}
