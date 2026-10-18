//! Level of detail (plan D4): which nodes of a tree a scene shows, within
//! the node budget, and the scene itself.

use std::collections::{HashMap, HashSet, VecDeque};

use rusqlite::Connection;

use super::{
    Bounds, Category, GRAPH_BUDGET, GRAPH_BUDGET_MAX, Layout, LegendEntry, Omitted, PAGE, Scene, SceneNode,
    SizeBy, TREE_BUDGET, TREE_BUDGET_MAX, ViewState, force, layout, now_ms,
};
use crate::Result;

pub const NONE: u32 = u32::MAX;

/// What a tree node stands for.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    Root,
    Url(i64),
    Dir(i64),
    Group(String),
}

impl Key {
    pub fn text(&self) -> String {
        match self {
            Key::Root => "root".into(),
            Key::Url(id) => format!("u:{id}"),
            Key::Dir(id) => format!("d:{id}"),
            Key::Group(g) => format!("g:{g}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub key: Key,
    pub parent: u32,
    /// In display order.
    pub children: Vec<u32>,
    pub category: Category,
    /// Crawl depth of a URL; tree level otherwise.
    pub depth: u32,
    /// Fixed label (folders, groups); URL labels are read for shown nodes.
    pub label: Option<String>,
    /// The URL to show for a page of the directory tree.
    pub url_id: Option<i64>,
    /// URLs this node stands for itself (1 for a URL, n for a page with n
    /// query strings, 0 for folders and groups).
    pub own: u32,
    /// URLs at or below this node; filled by [`Tree::finish`].
    pub count: u32,
    /// Nodes below this node; filled by [`Tree::finish`].
    pub descendants: u32,
    pub score: Option<f32>,
    pub inlinks: Option<u32>,
}

impl TreeNode {
    pub fn new(key: Key, category: Category) -> Self {
        Self {
            key,
            parent: NONE,
            children: Vec::new(),
            category,
            depth: 0,
            label: None,
            url_id: None,
            own: 0,
            count: 0,
            descendants: 0,
            score: None,
            inlinks: None,
        }
    }
}

/// A whole tree in memory: structure and numbers only, no URL text.
#[derive(Debug, Default)]
pub struct Tree {
    /// Index 0 is the root.
    pub nodes: Vec<TreeNode>,
}

impl Tree {
    pub fn push(&mut self, n: TreeNode) -> u32 {
        self.nodes.push(n);
        (self.nodes.len() - 1) as u32
    }

    pub fn attach(&mut self, child: u32, parent: u32) {
        self.nodes[child as usize].parent = parent;
        self.nodes[parent as usize].children.push(child);
    }

    /// Orders children (folders first and groups last; then strongest
    /// Link Score first when known, then as pushed) and fills counts
    /// bottom up.
    pub fn finish(&mut self) {
        let keys: Vec<(u8, f32)> = self
            .nodes
            .iter()
            .map(|n| {
                let rank = match n.category {
                    Category::Folder => 0,
                    Category::Group => 2,
                    _ => 1,
                };
                (rank, -n.score.unwrap_or(-1.0))
            })
            .collect();
        for n in &mut self.nodes {
            // Stable: equal keys keep their insertion order.
            n.children.sort_by(|&a, &b| {
                let (a, b) = (keys[a as usize], keys[b as usize]);
                a.0.cmp(&b.0).then(a.1.total_cmp(&b.1))
            });
        }
        // Children come after their parents in a breadth-first order, so a
        // reverse sweep sees every child before its parent.
        let order = self.bfs();
        for &i in order.iter().rev() {
            let (mut count, mut desc) = (self.nodes[i as usize].own, 0);
            for &c in &self.nodes[i as usize].children {
                count += self.nodes[c as usize].count;
                desc += 1 + self.nodes[c as usize].descendants;
            }
            let n = &mut self.nodes[i as usize];
            n.count = count;
            n.descendants = desc;
        }
    }

    fn bfs(&self) -> Vec<u32> {
        let mut order = Vec::with_capacity(self.nodes.len());
        if self.nodes.is_empty() {
            return order;
        }
        let mut q = VecDeque::from([0u32]);
        while let Some(i) = q.pop_front() {
            order.push(i);
            q.extend(self.nodes[i as usize].children.iter().copied());
        }
        order
    }

    /// URLs per category in the whole tree.
    fn legend(&self) -> Vec<LegendEntry> {
        let mut counts: HashMap<Category, u32> = HashMap::new();
        for n in &self.nodes {
            if n.own > 0 {
                *counts.entry(n.category).or_default() += n.own;
            }
        }
        Category::ALL
            .iter()
            .filter_map(|&c| {
                counts.get(&c).map(|&count| LegendEntry {
                    category: c,
                    label: c.label(),
                    count,
                })
            })
            .collect()
    }
}

/// A node as shown: a tree node, or "+N more" under one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    Node(u32),
    More { parent: u32, hidden: u32 },
}

/// The shown part of the tree: each shown node's shown children.
#[derive(Debug, Default)]
pub struct Visible {
    pub children: HashMap<u32, Vec<Shown>>,
    /// Open nodes.
    pub open: HashSet<u32>,
    pub count: u32,
}

/// The children a node shows when open: pages of [`PAGE`], then "+N more".
fn page(tree: &Tree, i: u32, pages: u32) -> Vec<Shown> {
    let all = &tree.nodes[i as usize].children;
    let n = (PAGE as usize * pages.max(1) as usize).min(all.len());
    let mut out: Vec<Shown> = all[..n].iter().map(|&c| Shown::Node(c)).collect();
    if n < all.len() {
        out.push(Shown::More {
            parent: i,
            hidden: (all.len() - n) as u32,
        });
    }
    out
}

/// Most nodes a scene may show: the tree budget, or the smaller graph
/// budget when the trees use the force layout (FR-019).
fn budget(state: &ViewState) -> u32 {
    let (default, max) = if state.layout == Layout::Force {
        (GRAPH_BUDGET, GRAPH_BUDGET_MAX)
    } else {
        (TREE_BUDGET, TREE_BUDGET_MAX)
    };
    if state.expanded.is_some() {
        max
    } else {
        state.budget.unwrap_or(default).clamp(1, max)
    }
}

/// Decides what to show (plan D4): breadth first from the root, a node
/// opens only if its shown children fit in what is left of the budget.
/// With `expanded`, exactly those nodes open, as the user asked, up to the
/// largest budget; otherwise any may, within the budget.
pub fn visible(tree: &Tree, state: &ViewState) -> Visible {
    let budget = budget(state);
    let wanted: Option<HashSet<&str>> = state
        .expanded
        .as_ref()
        .map(|e| e.iter().map(String::as_str).collect());
    let mut v = Visible {
        count: 1,
        ..Visible::default()
    };
    if tree.nodes.is_empty() {
        v.count = 0;
        return v;
    }
    let mut q = VecDeque::from([0u32]);
    while let Some(i) = q.pop_front() {
        let node = &tree.nodes[i as usize];
        if node.children.is_empty() {
            continue;
        }
        let key = node.key.text();
        // The root is always open when its children fit.
        if i != 0 && wanted.as_ref().is_some_and(|w| !w.contains(key.as_str())) {
            continue;
        }
        let pages = state.pages.get(&key).copied().unwrap_or(1);
        let kids = page(tree, i, pages);
        if v.count + kids.len() as u32 > budget {
            continue;
        }
        v.count += kids.len() as u32;
        for k in &kids {
            if let Shown::Node(c) = k {
                q.push_back(*c);
            }
        }
        v.open.insert(i);
        v.children.insert(i, kids);
    }
    v
}

/// Labels for the URL nodes being shown: the root URL in full, others by
/// path and query.
fn url_labels(conn: &Connection, ids: &[i64]) -> Result<HashMap<i64, String>> {
    let mut out = HashMap::with_capacity(ids.len());
    for chunk in ids.chunks(500) {
        let list = chunk.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
        let mut stmt = conn.prepare(&format!("SELECT id, url FROM urls WHERE id IN ({list})"))?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (id, url) = row?;
            out.insert(id, url);
        }
    }
    Ok(out)
}

/// The path and query of a URL, for its label.
pub(crate) fn path_label(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(u) => {
            let mut s = crate::sql::readable_url(url);
            if let Some(host) = u.host_str()
                && let Some(at) = s.find(host)
            {
                s = s[at + host.len()..].to_string();
                if let Some(port) = u.port() {
                    s = s.trim_start_matches(&format!(":{port}")).to_string();
                }
            }
            if s.is_empty() { "/".into() } else { s }
        }
        Err(_) => url.to_string(),
    }
}

fn radius(n: &TreeNode, size: SizeBy, scores: bool) -> f32 {
    if n.own == 0 && n.category != Category::Group {
        // Folders grow with the URLs below them.
        return (4.0 + 1.2 * (1.0 + n.count as f32).log2()).min(14.0);
    }
    match size {
        SizeBy::Score if scores => 3.0 + 0.9 * n.score.unwrap_or(0.0).max(0.0).sqrt(),
        SizeBy::Inlinks if scores => (3.0 + 1.5 * (1.0 + n.inlinks.unwrap_or(0) as f32).log2()).min(14.0),
        _ => 5.0,
    }
}

/// Force layout of the shown hierarchy (FR-009): parent-child edges,
/// starts seeded from the URL id (a hash of the key for folders and
/// groups), the root pinned at the centre.
fn force_positions(tree: &Tree, order: &[(Shown, u32, u32)], parents: &[u32]) -> (Vec<(f32, f32)>, bool) {
    let seed_id = |s: &Shown| -> i64 {
        match s {
            Shown::Node(i) => match tree.nodes[*i as usize].key {
                Key::Url(id) => id,
                ref key => fnv(&key.text()),
            },
            // "+N more" is keyed by its parent, like in the scene.
            Shown::More { parent, .. } => fnv(&format!("m:{}", tree.nodes[*parent as usize].key.text())),
        }
    };
    let start: Vec<(f64, f64)> = order
        .iter()
        .enumerate()
        .map(|(k, (s, _, _))| {
            if k == 0 {
                (0.0, 0.0)
            } else {
                force::seed_position(seed_id(s))
            }
        })
        .collect();
    let mut pinned = vec![false; order.len()];
    if let Some(p) = pinned.first_mut() {
        *p = true;
    }
    let edges: Vec<(u32, u32)> = parents
        .iter()
        .enumerate()
        .filter(|&(_, &p)| p != NONE)
        .map(|(k, &p)| (p, k as u32))
        .collect();
    let (pos, settled) = force::layout(start, &edges, &pinned);
    (
        pos.into_iter().map(|(x, y)| (x as f32, y as f32)).collect(),
        settled,
    )
}

/// FNV-1a, for seeds of nodes that have no URL id.
fn fnv(s: &str) -> i64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in s.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    (h >> 1) as i64
}

/// Builds the scene for `tree` under `state`.
pub fn scene(conn: &Connection, tree: &Tree, state: &ViewState, scores: bool) -> Result<Scene> {
    let vis = visible(tree, state);
    // Shown nodes in depth-first preorder, so parents come first.
    let mut order: Vec<(Shown, u32, u32)> = Vec::new(); // (node, parent index, level)
    if !tree.nodes.is_empty() {
        let mut stack = vec![(Shown::Node(0), NONE, 0u32)];
        while let Some((s, parent, level)) = stack.pop() {
            let me = order.len() as u32;
            order.push((s, parent, level));
            if let Shown::Node(i) = s
                && let Some(kids) = vis.children.get(&i)
            {
                for k in kids.iter().rev() {
                    stack.push((*k, me, level + 1));
                }
            }
        }
    }
    let parents: Vec<u32> = order.iter().map(|o| o.1).collect();
    let levels: Vec<u32> = order.iter().map(|o| o.2).collect();
    let mut settled = true;
    let pos = match state.layout {
        Layout::Tree | Layout::Cloud => layout::tidy(&parents, &levels),
        Layout::Radial => layout::radial(&parents, &levels),
        Layout::Force => {
            let (pos, done) = force_positions(tree, &order, &parents);
            settled = done;
            pos
        }
    };

    let ids: Vec<i64> = order
        .iter()
        .filter_map(|(s, _, _)| match s {
            Shown::Node(i) => match tree.nodes[*i as usize].key {
                Key::Url(id) => Some(id),
                _ => tree.nodes[*i as usize].url_id,
            },
            Shown::More { .. } => None,
        })
        .collect();
    let urls = url_labels(conn, &ids)?;

    let mut nodes = Vec::with_capacity(order.len());
    let mut shown_urls = 0u32;
    for (k, &(s, parent, _)) in order.iter().enumerate() {
        let (x, y) = pos[k];
        let parent = (parent != NONE).then_some(parent);
        match s {
            Shown::Node(i) => {
                let n = &tree.nodes[i as usize];
                shown_urls += n.own;
                let url_id = match n.key {
                    Key::Url(id) => Some(id),
                    _ => n.url_id,
                };
                let url = url_id.and_then(|id| urls.get(&id).cloned());
                let label = match (&n.label, &url) {
                    (Some(l), _) => l.clone(),
                    (None, Some(u)) if i == 0 => crate::sql::readable_url(u),
                    (None, Some(u)) => path_label(u),
                    (None, None) => n.key.text(),
                };
                let open = vis.open.contains(&i);
                nodes.push(SceneNode {
                    width: None,
                    key: n.key.text(),
                    label,
                    url,
                    url_id,
                    x,
                    y,
                    r: radius(n, state.size, scores),
                    category: n.category,
                    depth: n.depth,
                    score: n.score,
                    inlinks: n.inlinks,
                    count: n.count,
                    hidden: if open { 0 } else { n.descendants },
                    expandable: !n.children.is_empty(),
                    expanded: open,
                    parent,
                });
            }
            Shown::More { parent: p, hidden } => {
                let pk = tree.nodes[p as usize].key.text();
                nodes.push(SceneNode {
                    width: None,
                    key: format!("m:{pk}"),
                    label: format!("+{hidden} more"),
                    url: None,
                    url_id: None,
                    x,
                    y,
                    r: 5.0,
                    category: Category::Group,
                    depth: tree.nodes[p as usize].depth + 1,
                    score: None,
                    inlinks: None,
                    count: 0,
                    hidden,
                    expandable: true,
                    expanded: false,
                    parent,
                });
            }
        }
    }
    let total = tree.nodes.first().map_or(0, |n| n.count);
    let left = total.saturating_sub(shown_urls);
    let omitted = (left > 0).then(|| Omitted {
        count: left,
        reason: if vis.count >= budget(state) * 9 / 10 {
            "The rest are in folded branches: the view is near its node budget, set in Settings › Visualisations.".into()
        } else {
            "The rest are in folded branches: open one to see its URLs.".into()
        },
    });
    let size_legend = match state.size {
        SizeBy::Score if scores => Some("Size: Link Score".to_string()),
        SizeBy::Inlinks if scores => Some("Size: followed inlinks".to_string()),
        SizeBy::Score | SizeBy::Inlinks => {
            Some("Size: same for all (Link Score not worked out yet)".to_string())
        }
        SizeBy::Uniform => None,
    };
    Ok(Scene {
        note: None,
        language: None,
        view: state.view,
        layout: state.layout,
        bounds: bounds(&nodes),
        nodes,
        edges: Vec::new(),
        directed: false,
        settled,
        legend: tree.legend(),
        size_legend,
        omitted,
        total,
        snapshot_at: now_ms(),
    })
}

pub(crate) fn bounds(nodes: &[SceneNode]) -> Bounds {
    if nodes.is_empty() {
        return Bounds {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        };
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for n in nodes {
        x0 = x0.min(n.x - n.r);
        y0 = y0.min(n.y - n.r);
        x1 = x1.max(n.x + n.r);
        y1 = y1.max(n.y + n.r);
    }
    Bounds {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A root with `fan` children, each with `fan` children.
    fn two_levels(fan: usize) -> Tree {
        let mut t = Tree::default();
        let mut root = TreeNode::new(Key::Root, Category::Group);
        root.own = 0;
        t.push(root);
        let mut id = 1;
        for _ in 0..fan {
            let mut n = TreeNode::new(Key::Url(id), Category::Ok);
            n.own = 1;
            id += 1;
            let c = t.push(n);
            t.attach(c, 0);
            for _ in 0..fan {
                let mut g = TreeNode::new(Key::Url(id), Category::Ok);
                g.own = 1;
                id += 1;
                let gc = t.push(g);
                t.attach(gc, c);
            }
        }
        t.finish();
        t
    }

    #[test]
    fn counts_roll_up() {
        let t = two_levels(3);
        assert_eq!(t.nodes[0].count, 12);
        assert_eq!(t.nodes[0].descendants, 12);
        assert_eq!(t.nodes[1].count, 4);
        assert_eq!(t.nodes[1].descendants, 3);
    }

    #[test]
    fn initial_expansion_fits_the_budget() {
        let t = two_levels(10);
        let all = visible(&t, &ViewState::default());
        assert_eq!(all.count, 111);
        let state = ViewState {
            budget: Some(30),
            ..ViewState::default()
        };
        let v = visible(&t, &state);
        // Root (1) + 10 children, then only one child's 10 more fit (21),
        // a second would make 31.
        assert_eq!(v.count, 21);
        assert!(v.count <= 30);
        assert_eq!(v.open.len(), 2);
    }

    #[test]
    fn only_wanted_nodes_open() {
        let t = two_levels(4);
        let state = ViewState {
            expanded: Some(vec!["u:6".into()]),
            ..ViewState::default()
        };
        let v = visible(&t, &state);
        // Root always open; then only u:6 (the second child).
        assert_eq!(v.open.len(), 2);
        assert!(v.open.contains(&0));
        assert_eq!(
            t.nodes[*v.open.iter().find(|&&i| i != 0).unwrap() as usize].key,
            Key::Url(6)
        );
    }

    #[test]
    fn long_child_lists_page_with_a_more_node() {
        let mut t = Tree::default();
        t.push(TreeNode::new(Key::Root, Category::Group));
        for id in 0..1200 {
            let mut n = TreeNode::new(Key::Url(id), Category::Ok);
            n.own = 1;
            let c = t.push(n);
            t.attach(c, 0);
        }
        t.finish();
        let v = visible(&t, &ViewState::default());
        let kids = &v.children[&0];
        assert_eq!(kids.len(), 501);
        assert_eq!(
            kids[500],
            Shown::More {
                parent: 0,
                hidden: 700
            }
        );
        let state = ViewState {
            pages: [("root".to_string(), 3)].into(),
            ..ViewState::default()
        };
        let v = visible(&t, &state);
        assert_eq!(v.children[&0].len(), 1200, "all pages shown, no more node");
    }

    #[test]
    fn children_order_by_score_then_insertion() {
        let mut t = Tree::default();
        t.push(TreeNode::new(Key::Root, Category::Group));
        for (id, score) in [(1, None), (2, Some(10.0)), (3, Some(90.0)), (4, None)] {
            let mut n = TreeNode::new(Key::Url(id), Category::Ok);
            n.score = score;
            let c = t.push(n);
            t.attach(c, 0);
        }
        t.finish();
        let keys: Vec<_> = t.nodes[0]
            .children
            .iter()
            .map(|&c| t.nodes[c as usize].key.clone())
            .collect();
        assert_eq!(keys, [Key::Url(3), Key::Url(2), Key::Url(1), Key::Url(4)]);
    }

    #[test]
    fn path_labels() {
        assert_eq!(path_label("https://example.com/"), "/");
        assert_eq!(path_label("https://example.com/a/b?x=1"), "/a/b?x=1");
        assert_eq!(path_label("http://localhost:8080/a"), "/a");
    }
}
