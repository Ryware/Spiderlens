//! The directory tree (spec 022 US2, plan D3): the site's URL paths as
//! folders, with how many URLs each holds and how they fared.
//!
//! A folder is every path prefix ending in `/`; a page is the rest of the
//! path. URLs that differ only in their query string share one page node
//! with a count. A URL whose path ends in `/` is the page "/" of its
//! folder, so every URL is in exactly one page. Several hosts are top-level
//! branches.

use std::collections::HashMap;

use rusqlite::{Connection, params};

use super::lod::{Key, NONE, Tree, TreeNode};
use super::{CATEGORY_SQL, Category, TREE_URLS};
use crate::Result;

struct Folder {
    parent: Option<i64>,
    host: String,
    path: String,
    label: String,
    /// URLs at or below, per category code 0..=6.
    counts: [u32; 7],
}

/// Builds `temp.viz_dirs` (folders with rolled-up counts) and
/// `temp.viz_dir_urls` (each URL's folder and page).
pub fn build(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS temp.viz_dirs;
         DROP TABLE IF EXISTS temp.viz_dir_urls;
         CREATE TEMP TABLE viz_dirs (
           id INTEGER PRIMARY KEY, parent_id INTEGER, host TEXT NOT NULL, path TEXT NOT NULL,
           label TEXT NOT NULL, urls INTEGER NOT NULL, ok INTEGER NOT NULL, redirect INTEGER NOT NULL,
           client_error INTEGER NOT NULL, server_error INTEGER NOT NULL, blocked INTEGER NOT NULL,
           other INTEGER NOT NULL);
         CREATE TEMP TABLE viz_dir_urls (
           url_id INTEGER PRIMARY KEY, dir_id INTEGER NOT NULL, page_key TEXT NOT NULL);",
    )?;
    let mut folders: Vec<Folder> = Vec::new();
    let mut by_path: HashMap<(String, String), i64> = HashMap::new();
    let mut pages: Vec<(i64, i64, String)> = Vec::new();
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT u.id, u.url, {CATEGORY_SQL} FROM urls u WHERE {TREE_URLS} ORDER BY u.id"
        ))?;
        let mut q = stmt.query([])?;
        while let Some(r) = q.next()? {
            let id: i64 = r.get(0)?;
            let url: String = r.get(1)?;
            let cat: u8 = r.get(2)?;
            let Ok(u) = url::Url::parse(&url) else { continue };
            let host = match (u.host_str(), u.port()) {
                (Some(h), Some(p)) => format!("{h}:{p}"),
                (Some(h), None) => h.to_string(),
                (None, _) => continue,
            };
            let path = u.path();
            let cut = path.rfind('/').map_or(0, |i| i + 1);
            let (dir, page) = path.split_at(cut);
            let dir = if dir.is_empty() { "/" } else { dir };
            let page = if page.is_empty() { "/" } else { page };
            let d = folder_id(&host, dir, &mut folders, &mut by_path);
            // Roll the URL's category up through its folders.
            let mut at = Some(d);
            while let Some(f) = at {
                let f = &mut folders[f as usize - 1];
                f.counts[(cat as usize).min(6)] += 1;
                at = f.parent;
            }
            pages.push((id, d, page.to_string()));
        }
    }
    let tx = conn.unchecked_transaction()?;
    {
        let mut ins = tx.prepare(
            "INSERT INTO temp.viz_dirs VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )?;
        for (i, f) in folders.iter().enumerate() {
            let c = f.counts;
            let urls: u32 = c.iter().sum();
            // Not crawled (5) and other (6) share the "other" count.
            ins.execute(params![
                i as i64 + 1,
                f.parent,
                f.host,
                f.path,
                f.label,
                urls,
                c[0],
                c[1],
                c[2],
                c[3],
                c[4],
                c[5] + c[6]
            ])?;
        }
        let mut ins = tx.prepare("INSERT INTO temp.viz_dir_urls VALUES (?1, ?2, ?3)")?;
        for (id, d, page) in &pages {
            ins.execute(params![id, d, page])?;
        }
    }
    tx.commit()
}

/// The id of the folder `path` on `host`, adding it and its parents.
fn folder_id(
    host: &str,
    path: &str,
    folders: &mut Vec<Folder>,
    by_path: &mut HashMap<(String, String), i64>,
) -> i64 {
    if let Some(&id) = by_path.get(&(host.to_string(), path.to_string())) {
        return id;
    }
    let parent = (path != "/").then(|| {
        let trimmed = &path[..path.len() - 1];
        let cut = trimmed.rfind('/').map_or(0, |i| i + 1);
        trimmed[..cut].to_string()
    });
    let parent = parent.map(|p| folder_id(host, &p, folders, by_path));
    let id = folders.len() as i64 + 1;
    let label = if path == "/" {
        host.to_string()
    } else {
        let trimmed = &path[..path.len() - 1];
        format!("{}/", &trimmed[trimmed.rfind('/').map_or(0, |i| i + 1)..])
    };
    folders.push(Folder {
        parent,
        host: host.to_string(),
        path: path.to_string(),
        label,
        counts: [0; 7],
    });
    by_path.insert((host.to_string(), path.to_string()), id);
    id
}

/// Loads the directory tree: folders, then pages in URL order.
pub fn load(conn: &Connection, scores: bool) -> Result<Tree> {
    let mut tree = Tree::default();
    let mut root = TreeNode::new(Key::Root, Category::Group);
    root.label = Some("Crawl".into());
    tree.push(root);
    let mut index: HashMap<i64, u32> = HashMap::new();
    let mut parents: Vec<(u32, Option<i64>)> = Vec::new();
    {
        let mut stmt = conn.prepare("SELECT id, parent_id, label FROM viz_dirs ORDER BY host, path")?;
        let mut q = stmt.query([])?;
        while let Some(r) = q.next()? {
            let id: i64 = r.get(0)?;
            let mut n = TreeNode::new(Key::Dir(id), Category::Folder);
            n.label = Some(r.get(2)?);
            let i = tree.push(n);
            index.insert(id, i);
            parents.push((i, r.get(1)?));
        }
    }
    let mut tops = Vec::new();
    for (i, p) in parents {
        match p.and_then(|p| index.get(&p).copied()) {
            Some(p) => tree.attach(i, p),
            None => tops.push(i),
        }
    }
    let metrics = if scores {
        "m.score, m.followed_inlinks FROM viz_dir_urls d JOIN urls u ON u.id = d.url_id \
         LEFT JOIN link_metrics m ON m.url_id = d.url_id"
    } else {
        "NULL, NULL FROM viz_dir_urls d JOIN urls u ON u.id = d.url_id"
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT d.dir_id, d.page_key, d.url_id, u.depth, {CATEGORY_SQL}, {metrics} \
         ORDER BY d.dir_id, d.page_key, d.url_id"
    ))?;
    let mut q = stmt.query([])?;
    let mut last: Option<(i64, String, u32)> = None;
    while let Some(r) = q.next()? {
        let dir: i64 = r.get(0)?;
        let page: String = r.get(1)?;
        if let Some((d, p, i)) = &last
            && *d == dir
            && *p == page
        {
            // Same page, another query string.
            tree.nodes[*i as usize].own += 1;
            continue;
        }
        let url_id: i64 = r.get(2)?;
        let mut n = TreeNode::new(Key::Url(url_id), Category::from_code(r.get(4)?));
        n.depth = r.get(3)?;
        n.own = 1;
        n.url_id = Some(url_id);
        n.label = Some(page.clone());
        n.score = r.get::<_, Option<f64>>(5)?.map(|s| s as f32);
        n.inlinks = r.get(6)?;
        let i = tree.push(n);
        if let Some(&p) = index.get(&dir) {
            tree.attach(i, p);
        }
        last = Some((dir, page, i));
    }
    if tops.len() == 1 {
        // One host: its folder is the root.
        let host = tops[0];
        let mut nodes = std::mem::take(&mut tree.nodes);
        nodes.swap(0, host as usize);
        let fix = |x: u32| {
            if x == host {
                0
            } else if x == 0 {
                host
            } else {
                x
            }
        };
        for n in &mut nodes {
            n.parent = if n.parent == NONE { NONE } else { fix(n.parent) };
            for c in &mut n.children {
                *c = fix(*c);
            }
        }
        nodes[host as usize].children.clear();
        nodes[0].parent = NONE;
        let mut t = Tree { nodes };
        t.finish();
        return Ok(t);
    }
    for t in tops {
        tree.attach(t, 0);
    }
    tree.finish();
    Ok(tree)
}
