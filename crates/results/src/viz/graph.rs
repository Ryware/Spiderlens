//! The link graph (spec 022 US3, plan D4 and D5): internal HTML pages as
//! nodes, followed internal links between them as edges, laid out by
//! forces so tightly linked pages pull together.
//!
//! The whole-site graph shows the strongest pages up to the budget (by
//! Link Score, or by pages linking in when there is no score), always with
//! the start URL. Focus shows one page and its neighbours one or two links
//! away, nearest and strongest first.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

use super::lod::{bounds, path_label};
use super::{
    CATEGORY_SQL, Category, GRAPH_BUDGET, GRAPH_BUDGET_MAX, LegendEntry, Omitted, Scene, SceneEdge,
    SceneNode, SizeBy, ViewState, force, now_ms,
};
use crate::tabs::HTML_OK;
use crate::{Error, Result};

/// A followed anchor: what Link Score counts as passing value.
const FOLLOWED: &str = "l.kind = 'anchor' AND l.to_id IS NOT NULL AND l.from_id <> l.to_id \
    AND NOT (l.nofollow OR l.page_nofollow OR l.sponsored OR l.ugc)";

struct Page {
    id: i64,
    url: String,
    category: Category,
    depth: u32,
    score: Option<f32>,
    inlinks: u32,
}

/// Strength of every candidate page: Link Score when worked out, else the
/// number of pages linking to it with a followed link (one scan of the
/// links in source order, like Link Score's graph read).
fn strengths(conn: &Connection, scores: bool) -> Result<HashMap<i64, f64>> {
    let mut out = HashMap::new();
    if scores {
        let mut stmt = conn.prepare(&format!(
            "SELECT u.id, m.score FROM urls u JOIN link_metrics m ON m.url_id = u.id WHERE {HTML_OK}"
        ))?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?)))?;
        for row in rows {
            let (id, s) = row?;
            out.insert(id, s);
        }
        return Ok(out);
    }
    let mut stmt = conn.prepare(&format!("SELECT u.id FROM urls u WHERE {HTML_OK}"))?;
    let ids = stmt.query_map([], |r| r.get::<_, i64>(0))?;
    for id in ids {
        out.insert(id?, 0.0);
    }
    let mut stmt = conn.prepare(&format!(
        "SELECT l.from_id, l.to_id FROM links l WHERE {FOLLOWED} ORDER BY l.from_id"
    ))?;
    let mut q = stmt.query([])?;
    let (mut current, mut seen) = (None, Vec::<i64>::new());
    let flush = |seen: &mut Vec<i64>, out: &mut HashMap<i64, f64>| {
        seen.sort_unstable();
        seen.dedup();
        for t in seen.drain(..) {
            if let Some(s) = out.get_mut(&t) {
                *s += 1.0;
            }
        }
    };
    while let Some(r) = q.next()? {
        let from: i64 = r.get(0)?;
        if current != Some(from) {
            flush(&mut seen, &mut out);
            current = Some(from);
        }
        seen.push(r.get(1)?);
    }
    flush(&mut seen, &mut out);
    Ok(out)
}

/// Ids strongest first, ties by id.
fn strongest(ids: impl IntoIterator<Item = i64>, strength: &HashMap<i64, f64>) -> Vec<i64> {
    let mut v: Vec<i64> = ids.into_iter().collect();
    v.sort_by(|a, b| {
        let (sa, sb) = (
            strength.get(a).copied().unwrap_or(0.0),
            strength.get(b).copied().unwrap_or(0.0),
        );
        sb.total_cmp(&sa).then(a.cmp(b))
    });
    v
}

/// Followed neighbours of `id` among the candidates, both directions.
fn neighbours(conn: &Connection, id: i64, candidates: &HashMap<i64, f64>) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT l.to_id FROM links l WHERE l.from_id = ?1 AND {FOLLOWED}
         UNION SELECT l.from_id FROM links l WHERE l.to_id = ?1 AND {FOLLOWED}"
    ))?;
    let rows = stmt.query_map([id], |r| r.get::<_, i64>(0))?;
    let mut out = Vec::new();
    for row in rows {
        let n = row?;
        if candidates.contains_key(&n) {
            out.push(n);
        }
    }
    Ok(out)
}

fn start_url(conn: &Connection) -> Result<Option<i64>> {
    Ok(conn
        .query_row(
            &format!("SELECT u.id FROM urls u WHERE {HTML_OK} AND u.depth = 0 ORDER BY u.id LIMIT 1"),
            [],
            |r| r.get(0),
        )
        .ok())
}

/// Builds the link graph scene for `state`.
pub fn scene(conn: &Connection, state: &ViewState, scores: bool) -> Result<Scene> {
    let budget = state.budget.unwrap_or(GRAPH_BUDGET).clamp(1, GRAPH_BUDGET_MAX) as usize;
    let t0 = std::time::Instant::now();
    let lap = |l: &str| {
        if std::env::var_os("SPIDERLENS_TIMING").is_some() {
            eprintln!("{:>8.0} ms  link graph: {l}", t0.elapsed().as_secs_f64() * 1000.0);
        }
    };
    let strength = strengths(conn, scores)?;
    lap("strengths");
    let total = strength.len() as u32;
    let mut chosen: Vec<i64>;
    let focus = match &state.focus {
        Some(k) => Some(
            k.strip_prefix("u:")
                .and_then(|s| s.parse::<i64>().ok())
                .filter(|id| strength.contains_key(id))
                .ok_or_else(|| Error::invalid(format!("{k} is not an internal HTML page of this crawl")))?,
        ),
        None => None,
    };
    let mut ring: HashMap<i64, u8> = HashMap::new();
    match focus {
        Some(f) => {
            // Rings outwards, each strongest first, within the budget.
            chosen = vec![f];
            ring.insert(f, 0);
            let mut frontier = vec![f];
            for d in 1..=state.distance.clamp(1, 2) {
                let mut next: HashSet<i64> = HashSet::new();
                for &p in &frontier {
                    for n in neighbours(conn, p, &strength)? {
                        if !ring.contains_key(&n) {
                            next.insert(n);
                        }
                    }
                }
                let next = strongest(next, &strength);
                let room = budget.saturating_sub(chosen.len());
                for &n in next.iter().take(room) {
                    ring.insert(n, d);
                    chosen.push(n);
                }
                frontier = next.into_iter().take(room).collect();
            }
        }
        None => {
            chosen = strongest(strength.keys().copied(), &strength);
            chosen.truncate(budget);
            if let Some(s) = start_url(conn)?
                && !chosen.contains(&s)
            {
                chosen.pop();
                chosen.push(s);
            }
            chosen.sort_unstable();
        }
    }
    let index: HashMap<i64, u32> = chosen.iter().enumerate().map(|(i, &id)| (id, i as u32)).collect();

    // Page details and the edges among the chosen pages, through a temp
    // table of their ids.
    conn.execute_batch(
        "DROP TABLE IF EXISTS temp.viz_sel; CREATE TEMP TABLE viz_sel (id INTEGER PRIMARY KEY);",
    )?;
    {
        let tx = conn.unchecked_transaction()?;
        {
            let mut ins = tx.prepare("INSERT INTO temp.viz_sel VALUES (?1)")?;
            for &id in &chosen {
                ins.execute([id])?;
            }
        }
        tx.commit()?;
    }
    let metrics = if scores {
        "m.score FROM temp.viz_sel s JOIN urls u ON u.id = s.id LEFT JOIN link_metrics m ON m.url_id = s.id"
    } else {
        "NULL FROM temp.viz_sel s JOIN urls u ON u.id = s.id"
    };
    let mut pages: Vec<Option<Page>> = (0..chosen.len()).map(|_| None).collect();
    {
        let mut stmt = conn.prepare(&format!("SELECT u.id, u.url, u.depth, {CATEGORY_SQL}, {metrics}"))?;
        let rows = stmt.query_map([], |r| {
            Ok(Page {
                id: r.get(0)?,
                url: r.get(1)?,
                depth: r.get(2)?,
                category: Category::from_code(r.get(3)?),
                score: r.get::<_, Option<f64>>(4)?.map(|s| s as f32),
                inlinks: 0,
            })
        })?;
        for row in rows {
            let p = row?;
            let i = index[&p.id] as usize;
            pages[i] = Some(p);
        }
    }
    let mut edges: Vec<(u32, u32)> = Vec::new();
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT DISTINCT l.from_id, l.to_id FROM temp.viz_sel s JOIN links l ON l.from_id = s.id
             JOIN temp.viz_sel t ON t.id = l.to_id WHERE {FOLLOWED} ORDER BY l.from_id, l.to_id"
        ))?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        for row in rows {
            let (a, b) = row?;
            edges.push((index[&a], index[&b]));
        }
    }
    conn.execute_batch("DROP TABLE IF EXISTS temp.viz_sel")?;
    for &(_, b) in &edges {
        if let Some(p) = pages[b as usize].as_mut() {
            p.inlinks += 1;
        }
    }

    // Layout: seeded starts, the focus page fixed at the centre, dragged
    // nodes where the user left them.
    let mut pinned = vec![false; chosen.len()];
    let start: Vec<(f64, f64)> = chosen
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            if let Some(&[x, y]) = state.pinned.get(&format!("u:{id}")) {
                pinned[i] = true;
                return (f64::from(x), f64::from(y));
            }
            if Some(id) == focus {
                pinned[i] = true;
                return (0.0, 0.0);
            }
            force::seed_position(id)
        })
        .collect();
    lap("pages and edges");
    let (pos, settled) = force::layout(start, &edges, &pinned);
    lap("layout");

    let nodes: Vec<SceneNode> = pages
        .into_iter()
        .zip(pos)
        .map(|(p, (x, y))| {
            let p = p.expect("every chosen page was read");
            let r = match state.size {
                SizeBy::Score if scores => 3.0 + 0.9 * p.score.unwrap_or(0.0).max(0.0).sqrt(),
                SizeBy::Inlinks => (3.0 + 1.5 * (1.0 + p.inlinks as f32).log2()).min(14.0),
                _ => 5.0,
            };
            SceneNode {
                width: None,
                key: format!("u:{}", p.id),
                label: path_label(&p.url),
                url: Some(p.url),
                url_id: Some(p.id),
                x: x as f32,
                y: y as f32,
                r,
                category: p.category,
                depth: ring.get(&p.id).map_or(p.depth, |&d| u32::from(d)),
                score: p.score,
                inlinks: Some(p.inlinks),
                count: 1,
                hidden: 0,
                expandable: false,
                expanded: false,
                parent: None,
            }
        })
        .collect();
    let mut counts: HashMap<Category, u32> = HashMap::new();
    for n in &nodes {
        *counts.entry(n.category).or_default() += 1;
    }
    let legend = Category::ALL
        .iter()
        .filter_map(|&c| {
            counts.get(&c).map(|&count| LegendEntry {
                category: c,
                label: c.label(),
                count,
            })
        })
        .collect();
    let left = total.saturating_sub(nodes.len() as u32);
    let omitted = (left > 0).then(|| Omitted {
        count: left,
        reason: match focus {
            Some(_) => {
                "Pages further away are not drawn: choose distance 2, or focus on another page.".into()
            }
            None if scores => format!(
                "Showing the {} strongest by Link Score: focus on a page to see the rest.",
                nodes.len()
            ),
            None => format!(
                "Showing the {} with the most pages linking in: focus on a page to see the rest.",
                nodes.len()
            ),
        },
    });
    let size_legend = match state.size {
        SizeBy::Score if scores => Some("Size: Link Score".to_string()),
        SizeBy::Score => Some("Size: same for all (Link Score not worked out yet)".to_string()),
        SizeBy::Inlinks => Some("Size: links in from the pages shown".to_string()),
        SizeBy::Uniform => None,
    };
    Ok(Scene {
        note: None,
        language: None,
        view: state.view,
        layout: super::Layout::Force,
        bounds: bounds(&nodes),
        edges: edges.into_iter().map(|(a, b)| SceneEdge { a, b }).collect(),
        directed: focus.is_some(),
        settled,
        nodes,
        legend,
        size_legend,
        omitted,
        total,
        snapshot_at: now_ms(),
    })
}
