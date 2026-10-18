//! Site visualisations (spec 022): every view is a [`Scene`] built here,
//! drawn by the desktop app on a canvas and written by the CLI as SVG, so
//! the two always agree (plan D1).
//!
//! The crawl tree and the directory tree come from two derived tables built
//! in the reader's `temp` schema on demand (stage 1, plan D2 and D3). Each
//! scene shows at most a budget of nodes: branches that do not fit stay
//! folded, and long child lists show a page at a time behind a "+N more"
//! node (plan D4).

pub mod anchors;
pub mod crawl_tree;
pub mod csv;
pub mod dirs;
pub mod export;
pub mod force;
pub mod graph;
pub mod layout;
pub mod lod;
pub mod palette;
pub mod png;
pub mod svg;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Result;
use crate::reader::Reader;
use crate::tabs::{Derived, Need};

/// Default and largest node budgets for the trees (plan D4).
pub const TREE_BUDGET: u32 = 20_000;
pub const TREE_BUDGET_MAX: u32 = 50_000;
/// Children shown per page under a folded node.
pub const PAGE: u32 = 500;
/// Default and largest node budgets for the link graph (plan D4).
pub const GRAPH_BUDGET: u32 = 5_000;
pub const GRAPH_BUDGET_MAX: u32 = 15_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ViewKind {
    CrawlTree,
    DirectoryTree,
    LinkGraph,
    /// Anchor text cloud for a page (`focus`) or the whole site.
    Anchors,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Layout {
    #[default]
    Tree,
    Radial,
    /// Force-directed; the link graph always uses it.
    Force,
    /// Word cloud; the anchor text view always uses it.
    Cloud,
}

/// What a term of an anchor text cloud is (FR-016).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Terms {
    #[default]
    Words,
    /// The whole anchor text, whitespace collapsed and lower case.
    Whole,
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
    /// Most nodes to show; None for [`TREE_BUDGET`] or [`GRAPH_BUDGET`].
    pub budget: Option<u32>,
    /// Link graph: the page to centre on (`u:<id>`), with its neighbours
    /// up to `distance` links away (1 or 2). Anchor text: the page whose
    /// inlinks make the cloud. None: the whole site.
    pub focus: Option<String>,
    pub distance: u8,
    /// Link graph: nodes the user dragged, by key, and where to.
    #[ts(type = "Record<string, [number, number]>")]
    pub pinned: BTreeMap<String, [f32; 2]>,
    /// Anchor text: words or whole anchors, stop words left out, and
    /// followed links only (FR-016, FR-017).
    pub terms: Terms,
    pub stop_words: bool,
    pub followed_only: bool,
    /// Anchor text: count only links at these positions (spec 023: the
    /// `Position` names and "unknown"); empty for all (FR-016). Ignored,
    /// with a note, when Crawl Analysis has not run.
    pub positions: Vec<String>,
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
            focus: None,
            distance: 1,
            pinned: BTreeMap::new(),
            terms: Terms::Words,
            stop_words: true,
            followed_only: false,
            positions: Vec::new(),
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
    /// Radius; for an anchor term, half its height.
    pub r: f32,
    /// Anchor terms: the width of the text box, centred on (x, y).
    pub width: Option<f32>,
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

/// A followed link between two shown pages (link graph).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct SceneEdge {
    /// Indexes in `nodes`: from `a` to `b`.
    pub a: u32,
    pub b: u32,
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
    /// Link graph: followed links among the shown pages. Trees draw
    /// parent lines instead.
    pub edges: Vec<SceneEdge>,
    /// Edges have a direction worth drawing (focus mode).
    pub directed: bool,
    /// The force layout converged (always true for trees).
    pub settled: bool,
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
    /// Said instead of an empty picture (anchor text with nothing to show).
    pub note: Option<String>,
    /// Anchor text: the language whose stop words apply.
    pub language: Option<String>,
}

impl Reader {
    /// Builds the scene for `state` (spec 022 FR-001 to FR-008).
    pub fn viz_scene(&mut self, state: &ViewState) -> Result<Scene> {
        let derived = match state.view {
            ViewKind::CrawlTree => Derived::VizCrawlTree,
            ViewKind::DirectoryTree => Derived::VizDirs,
            ViewKind::LinkGraph => {
                let scores = self.has_derived(Derived::LinkMetrics)?;
                return graph::scene(&self.conn, state, scores);
            }
            ViewKind::Anchors => {
                let (positions, ignored) = self.viz_positions(state)?;
                let mut scene = anchors::scene(&self.conn, state, &positions)?;
                if let Some(why) = ignored {
                    let legend = scene.size_legend.get_or_insert_with(String::new);
                    legend.push_str(&format!(". Position filter ignored: {why}"));
                }
                return Ok(scene);
            }
        };
        self.ensure_derived([derived])?;
        // Link Score sizes and orders nodes only once worked out: a view
        // never waits for the analysis.
        let scores = self.has_derived(Derived::LinkMetrics)?;
        let tree = match state.view {
            ViewKind::CrawlTree => crawl_tree::load(&self.conn, scores)?,
            ViewKind::DirectoryTree => dirs::load(&self.conn, scores)?,
            ViewKind::LinkGraph | ViewKind::Anchors => unreachable!("handled above"),
        };
        lod::scene(&self.conn, &tree, state, scores)
    }
}

impl Reader {
    /// The links behind one term of an anchor text cloud (FR-018).
    pub fn viz_term_links(
        &mut self,
        state: &ViewState,
        term: &str,
        limit: usize,
    ) -> Result<anchors::TermLinks> {
        let (positions, _) = self.viz_positions(state)?;
        anchors::term_links(&self.conn, state, &positions, term, limit)
    }

    /// The position filter to apply (checked names, empty for none) and,
    /// when the user asked for one that cannot apply, why it is ignored.
    /// Builds the link positions when they are needed (FR-016).
    fn viz_positions(&mut self, state: &ViewState) -> Result<(Vec<String>, Option<&'static str>)> {
        if state.positions.is_empty() {
            return Ok((Vec::new(), None));
        }
        for p in &state.positions {
            if p != "unknown" && !spiderlens_config::Position::ALL.iter().any(|x| x.as_str() == p) {
                return Err(crate::Error::invalid(format!("unknown position {p}")));
            }
        }
        if !self.features.has(Need::Analysis) {
            return Ok((Vec::new(), Some(Need::Analysis.unavailable_reason())));
        }
        self.ensure_derived([Derived::LinkPositions])?;
        let mut positions = state.positions.clone();
        positions.sort();
        positions.dedup();
        Ok((positions, None))
    }
}

/// `1711179` → `1,711,179`, for text in scenes and exports.
pub fn thousands(v: u64) -> String {
    let d = v.to_string();
    let mut out = String::with_capacity(d.len() + d.len() / 3);
    for (i, c) in d.chars().enumerate() {
        if i > 0 && (d.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Milliseconds since the epoch, for `snapshot_at`.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

#[cfg(test)]
mod tests {
    #[test]
    fn thousands_groups_digits() {
        assert_eq!(super::thousands(0), "0");
        assert_eq!(super::thousands(999), "999");
        assert_eq!(super::thousands(1000), "1,000");
        assert_eq!(super::thousands(1_711_179), "1,711,179");
    }
}
