//! The crawl tree (spec 022 US1, plan D2): each URL under the page that
//! first reached it at the depth it has now.
//!
//! The crawl keeps the first discoverer in `urls.source_id` even when a
//! shorter path turns up later, so the parent is worked out: the source
//! when it sits one level up, else the earliest-discovered page one level
//! up that links or redirects to the URL. URLs found only through sitemaps,
//! canonicals, hreflang, images or the URL list sit under "Found outside
//! links", one group per source.

use rusqlite::{Connection, OptionalExtension, params};

use super::lod::{Key, NONE, Tree, TreeNode};
use super::{CATEGORY_SQL, Category, TREE_URLS};
use crate::Result;

/// Builds `temp.viz_crawl_tree(url_id, parent_id, grp)`: `parent_id` NULL
/// and `grp` NULL for top-level URLs (depth 0), `grp` the discovery source
/// for URLs found outside links.
pub fn build(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS temp.viz_crawl_tree;
         CREATE TEMP TABLE viz_crawl_tree (
           url_id INTEGER PRIMARY KEY, parent_id INTEGER, grp TEXT);",
    )?;
    let max_id: i64 = conn.query_row("SELECT COALESCE(MAX(id), 0) FROM urls", [], |r| r.get(0))?;
    // Depth of each tree URL by id; -1 = not in the tree.
    let mut depth = vec![-1i64; usize::try_from(max_id).unwrap_or(0) + 1];
    let mut rows: Vec<(i64, i64, String, Option<i64>)> = Vec::new();
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT u.id, u.depth, u.source, u.source_id FROM urls u WHERE {TREE_URLS} ORDER BY u.id"
        ))?;
        let mut q = stmt.query([])?;
        while let Some(r) = q.next()? {
            let row: (i64, i64, String, Option<i64>) = (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?);
            depth[row.0 as usize] = row.1;
            rows.push(row);
        }
    }
    let at = |id: i64| depth.get(id as usize).copied().unwrap_or(-1);
    // The source is the parent when it sits one level up, as it does for
    // most URLs of a breadth-first crawl.
    let mut parent: Vec<Option<i64>> = rows
        .iter()
        .map(|(_, d, _, source_id)| source_id.filter(|&s| *d > 0 && at(s) == d - 1))
        .collect();
    let need: Vec<usize> = (0..rows.len())
        .filter(|&k| parent[k].is_none() && rows[k].1 > 0)
        .collect();
    let pick = "SELECT from_id, to_id FROM links \
         WHERE kind IN ('anchor', 'redirect', 'meta_refresh') AND to_id IS NOT NULL AND from_id <> to_id";
    if need.len() <= 2_000 {
        // Few: one indexed probe each, earliest-discovered source first.
        let mut probe = conn.prepare(&format!("{pick} AND to_id = ?1 ORDER BY from_id"))?;
        for &k in &need {
            let (id, d, _, _) = &rows[k];
            let mut q = probe.query([id])?;
            while let Some(r) = q.next()? {
                let from: i64 = r.get(0)?;
                if at(from) == d - 1 {
                    parent[k] = Some(from);
                    break;
                }
            }
        }
    } else {
        // Many: one scan of the links, keeping the smallest fitting source.
        let mut slot = vec![u32::MAX; depth.len()];
        for &k in &need {
            slot[rows[k].0 as usize] = k as u32;
        }
        let mut stmt = conn.prepare(pick)?;
        let mut q = stmt.query([])?;
        while let Some(r) = q.next()? {
            let (from, to): (i64, i64) = (r.get(0)?, r.get(1)?);
            let Some(&k) = slot.get(to as usize).filter(|&&k| k != u32::MAX) else {
                continue;
            };
            let k = k as usize;
            if at(from) == rows[k].1 - 1 && parent[k].is_none_or(|p| from < p) {
                parent[k] = Some(from);
            }
        }
    }
    let tx = conn.unchecked_transaction()?;
    {
        let mut ins = tx.prepare("INSERT INTO temp.viz_crawl_tree VALUES (?1, ?2, ?3)")?;
        for (k, (id, d, source, _)) in rows.iter().enumerate() {
            match parent[k] {
                Some(p) => ins.execute(params![id, p, None::<String>])?,
                None if *d == 0 && (source == "start" || source == "list") => {
                    ins.execute(params![id, None::<i64>, None::<String>])?
                }
                None => ins.execute(params![id, None::<i64>, source])?,
            };
        }
    }
    tx.commit()
}

/// The label of a "Found outside links" group.
fn group_label(source: &str) -> String {
    match source {
        "sitemap" => "From sitemaps".into(),
        "canonical" => "From canonicals".into(),
        "hreflang" => "From hreflang".into(),
        "image" => "From images".into(),
        "list" => "From the URL list".into(),
        "redirect" => "From redirects".into(),
        "start" => "Start URLs".into(),
        _ => "Other".into(),
    }
}

/// Loads the crawl tree. With `scores`, `link_metrics` is current and
/// orders children and sizes nodes.
pub fn load(conn: &Connection, scores: bool) -> Result<Tree> {
    let metrics = if scores {
        "m.score, m.followed_inlinks FROM viz_crawl_tree t JOIN urls u ON u.id = t.url_id \
         LEFT JOIN link_metrics m ON m.url_id = t.url_id"
    } else {
        "NULL, NULL FROM viz_crawl_tree t JOIN urls u ON u.id = t.url_id"
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT t.url_id, t.parent_id, t.grp, u.depth, {CATEGORY_SQL}, {metrics} ORDER BY t.url_id"
    ))?;
    let mut tree = Tree::default();
    tree.push(TreeNode::new(Key::Root, Category::Group));
    let max_id: i64 = conn
        .query_row("SELECT MAX(url_id) FROM viz_crawl_tree", [], |r| r.get(0))
        .optional()?
        .flatten()
        .unwrap_or(0);
    let mut index = vec![NONE; usize::try_from(max_id).unwrap_or(0) + 1];
    let mut links: Vec<(u32, Option<i64>, Option<String>)> = Vec::new();
    let mut q = stmt.query([])?;
    while let Some(r) = q.next()? {
        let id: i64 = r.get(0)?;
        let mut n = TreeNode::new(Key::Url(id), Category::from_code(r.get(4)?));
        n.depth = r.get(3)?;
        n.own = 1;
        n.score = r.get::<_, Option<f64>>(5)?.map(|s| s as f32);
        n.inlinks = r.get(6)?;
        let i = tree.push(n);
        index[id as usize] = i;
        links.push((i, r.get(1)?, r.get(2)?));
    }
    let mut groups: Vec<(String, u32)> = Vec::new();
    let mut outside = NONE;
    let mut tops = Vec::new();
    for (i, parent, grp) in links {
        match (parent, grp) {
            (Some(p), _) => match index.get(p as usize).copied().filter(|&x| x != NONE) {
                Some(p) => tree.attach(i, p),
                None => tops.push(i),
            },
            (None, Some(g)) => {
                if outside == NONE {
                    let mut o = TreeNode::new(Key::Group("outside".into()), Category::Group);
                    o.label = Some("Found outside links".into());
                    outside = tree.push(o);
                }
                let gi = match groups.iter().find(|(name, _)| *name == g) {
                    Some(&(_, gi)) => gi,
                    None => {
                        let mut n = TreeNode::new(Key::Group(g.clone()), Category::Group);
                        n.label = Some(group_label(&g));
                        let gi = tree.push(n);
                        groups.push((g, gi));
                        gi
                    }
                };
                tree.attach(i, gi);
            }
            (None, None) => tops.push(i),
        }
    }
    // One start URL and nothing found outside links: it is the root.
    if tops.len() == 1 && outside == NONE {
        let start = tops[0];
        let mut nodes = std::mem::take(&mut tree.nodes);
        // Swap the start URL into slot 0 and drop the synthetic root.
        nodes.swap(0, start as usize);
        let mut t = Tree { nodes };
        let fix = |x: u32| {
            if x == start {
                0
            } else if x == 0 {
                start
            } else {
                x
            }
        };
        for n in &mut t.nodes {
            n.parent = if n.parent == NONE { NONE } else { fix(n.parent) };
            for c in &mut n.children {
                *c = fix(*c);
            }
        }
        // The old root is now an unattached node at `start`: make it an
        // empty leaf nobody points to and keep counts right.
        let old = &mut t.nodes[start as usize];
        old.own = 0;
        old.children.clear();
        t.nodes[0].parent = NONE;
        t.finish();
        return Ok(t);
    }
    let mut root = std::mem::replace(&mut tree.nodes[0], TreeNode::new(Key::Root, Category::Group));
    root.label = Some("Crawl".into());
    tree.nodes[0] = root;
    for t in tops {
        tree.attach(t, 0);
    }
    if outside != NONE {
        for &(_, gi) in &groups {
            tree.attach(gi, outside);
        }
        tree.attach(outside, 0);
    }
    tree.finish();
    Ok(tree)
}
