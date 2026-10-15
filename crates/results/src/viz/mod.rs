//! Site visualisations (spec 022): every view is a [`Scene`] built here,
//! drawn by the desktop app on a canvas and written by the CLI as SVG, so
//! the two always agree (plan D1).
//!
//! The crawl tree and the directory tree come from two derived tables built
//! in the reader's `temp` schema on demand (stage 1, plan D2 and D3). Each
//! scene shows at most a budget of nodes: branches that do not fit stay
//! folded, and long child lists show a page at a time behind a "+N more"
//! node (plan D4).

pub mod crawl_tree;
pub mod dirs;
pub mod layout;
pub mod lod;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Result;
use crate::reader::Reader;
use crate::tabs::Derived;

/// Default and largest node budgets for the trees (plan D4).
pub const TREE_BUDGET: u32 = 20_000;
pub const TREE_BUDGET_MAX: u32 = 50_000;
/// Children shown per page under a folded node.
pub const PAGE: u32 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ViewKind {
    CrawlTree,
    DirectoryTree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Layout {
    #[default]
    Tree,
    Radial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ColourBy {
    #[default]
    Status,
    Depth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SizeBy {
    #[default]
    Uniform,
    /// Link Score (spec 023); uniform when it has not been worked out.
    Score,
    Inlinks,
}

/// What the user asked to see (FR-007). The same state gives the same
/// scene, on screen and in an export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ViewState {
    pub view: ViewKind,
    pub layout: Layout,
    pub colour: ColourBy,
    pub size: SizeBy,
    /// Keys of the open nodes. None: the initial expansion, breadth first
    /// within the budget.
    pub expanded: Option<Vec<String>>,
    /// Pages of children shown under a node, by node key (default 1).
    pub pages: BTreeMap<String, u32>,
    /// Most nodes to show; None for [`TREE_BUDGET`].
    pub budget: Option<u32>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            view: ViewKind::CrawlTree,
            layout: Layout::Tree,
            colour: ColourBy::Status,
            size: SizeBy::Uniform,
            expanded: None,
            pages: BTreeMap::new(),
            budget: None,
        }
    }
}

/// How a node is coloured and shaped (FR-005: shape as well as colour).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Category {
    Ok,
    Redirect,
    ClientError,
    /// 5xx, or no response at all (timeout, connection failed).
    ServerError,
    Blocked,
    NotCrawled,
    Other,
    Folder,
    /// A grouping node: the crawl root, "Found outside links", "+N more".
    Group,
}

impl Category {
    pub const ALL: [Category; 9] = [
        Self::Ok,
        Self::Redirect,
        Self::ClientError,
        Self::ServerError,
        Self::Blocked,
        Self::NotCrawled,
        Self::Other,
        Self::Folder,
        Self::Group,
    ];

    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(c: u8) -> Self {
        Self::ALL.get(c as usize).copied().unwrap_or(Self::Other)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ok => "Success (2xx)",
            Self::Redirect => "Redirect (3xx)",
            Self::ClientError => "Client error (4xx)",
            Self::ServerError => "Server error or no response",
            Self::Blocked => "Blocked by robots.txt",
            Self::NotCrawled => "Not crawled",
            Self::Other => "Other",
            Self::Folder => "Folder",
            Self::Group => "Group",
        }
    }
}

/// SQL giving a URL's [`Category`] code, on `urls u`.
pub(crate) const CATEGORY_SQL: &str = "CASE \
    WHEN u.state = 'not_crawled' AND u.reason IN ('blocked_by_robots', 'robots_unavailable') THEN 4 \
    WHEN u.state <> 'crawled' THEN 5 \
    WHEN u.error_class IS NOT NULL THEN 3 \
    WHEN u.status BETWEEN 200 AND 299 THEN 0 \
    WHEN u.status BETWEEN 300 AND 399 THEN 1 \
    WHEN u.status BETWEEN 400 AND 499 THEN 2 \
    WHEN u.status BETWEEN 500 AND 599 THEN 3 \
    ELSE 6 END";

/// The URLs the trees show: internal ones that were crawled, or were
/// meant to be and were held back (queued, blocked, over a limit).
pub(crate) const TREE_URLS: &str = "u.internal = 1 AND (u.state <> 'not_crawled' OR u.reason IN \
    ('blocked_by_robots', 'robots_unavailable', 'over_url_limit', 'over_depth_limit', 'crawl_stopped'))";

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SceneNode {
    /// Stable across scenes: `u:<url id>`, `d:<folder id>`, `g:<group>`,
    /// `root`, or `m:<parent key>` for "+N more".
    pub key: String,
    pub label: String,
    /// The URL, for URL nodes.
    pub url: Option<String>,
    #[ts(type = "number | null")]
    pub url_id: Option<i64>,
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub category: Category,
    pub depth: u32,
    pub score: Option<f32>,
    pub inlinks: Option<u32>,
    /// URLs at or below this node (a folder's pages, a branch's URLs).
    pub count: u32,
    /// Nodes folded into this one: its hidden descendants, or for "+N
    /// more", the children not yet shown.
    pub hidden: u32,
    /// Has children that can be shown or hidden.
    pub expandable: bool,
    pub expanded: bool,
    /// Index of the parent in `nodes`; None for the root.
    pub parent: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[ts(export)]
pub struct Bounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct LegendEntry {
    pub category: Category,
    pub label: &'static str,
    /// URLs of this category in the whole tree, shown or not.
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Omitted {
    pub count: u32,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Scene {
    pub view: ViewKind,
    pub layout: Layout,
    /// Drawn in this order; a node's parent always comes first.
    pub nodes: Vec<SceneNode>,
    pub legend: Vec<LegendEntry>,
    /// What the size of a node means, when it means something.
    pub size_legend: Option<String>,
    pub omitted: Option<Omitted>,
    pub bounds: Bounds,
    /// URLs in the tree, shown or not.
    pub total: u32,
    /// When the data was read (ms since epoch): views do not follow a
    /// running crawl on their own (FR-006).
    #[ts(type = "number")]
    pub snapshot_at: i64,
}

impl Reader {
    /// Builds the scene for `state` (spec 022 FR-001 to FR-008).
    pub fn viz_scene(&mut self, state: &ViewState) -> Result<Scene> {
        let derived = match state.view {
            ViewKind::CrawlTree => Derived::VizCrawlTree,
            ViewKind::DirectoryTree => Derived::VizDirs,
        };
        self.ensure_derived([derived])?;
        // Link Score sizes and orders nodes only once worked out: a view
        // never waits for the analysis.
        let scores = self.has_derived(Derived::LinkMetrics)?;
        let tree = match state.view {
            ViewKind::CrawlTree => crawl_tree::load(&self.conn, scores)?,
            ViewKind::DirectoryTree => dirs::load(&self.conn, scores)?,
        };
        lod::scene(&self.conn, &tree, state, scores)
    }
}

/// Milliseconds since the epoch, for `snapshot_at`.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}
