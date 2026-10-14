//! Link Score and inlink counts for every internal URL (spec 023, plan D2-D4).
//!
//! The internal link graph is read once, in link order, into compressed
//! sparse rows (one offset per node, one target per distinct edge), then
//! ranked with damped power iteration. One thread and a fixed summation
//! order make the result bit-for-bit the same on every platform (FR-005).

use rusqlite::{Connection, params};

use super::Watch;
use spiderlens_config::PositionWeights;

/// The damping factor of FR-001.
pub const DAMPING: f64 = 0.85;
/// Iteration stops when the total change between rounds is below this...
const TOLERANCE: f64 = 1e-9;
/// ...or after this many rounds.
const MAX_ROUNDS: u32 = 100;

const NONE: u32 = u32::MAX;

/// What a run looked at, for the run record and logs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    pub urls: u64,
    pub edges: u64,
    pub rounds: u32,
}

/// The internal link graph in compressed sparse rows: node `i`'s distinct
/// out-edges are `targets[offsets[i]..offsets[i + 1]]`. The link counts of
/// FR-006 are gathered in the same scan.
#[derive(Debug, Default)]
pub struct Graph {
    /// `urls.id` of each node, ascending.
    pub ids: Vec<i64>,
    pub offsets: Vec<u32>,
    pub targets: Vec<u32>,
    /// With position weighting on (FR-025): each edge's weight, the highest
    /// among the links forming it, and each node's total. Empty when off.
    pub weights: Vec<f32>,
    pub out_weight: Vec<f64>,
    /// Distinct internal pages with a followed anchor to each node
    /// (redirect edges are not anchors and are not counted).
    pub followed_in: Vec<u32>,
    /// Distinct internal pages with any anchor to each node.
    pub unique_in: Vec<u32>,
    /// Every anchor to each node.
    pub all_in: Vec<u32>,
    /// Distinct internal targets of each node's anchors.
    pub unique_out: Vec<u32>,
    /// Distinct external targets of each node's anchors.
    pub external_out: Vec<u32>,
}

impl Graph {
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    fn out(&self, i: usize) -> &[u32] {
        &self.targets[self.offsets[i] as usize..self.offsets[i + 1] as usize]
    }

    /// A node that passes nothing on: its rank is spread over every node.
    fn dangling(&self, i: usize) -> bool {
        if self.weights.is_empty() {
            self.offsets[i] == self.offsets[i + 1]
        } else {
            self.out_weight[i] <= 0.0
        }
    }
}

/// One source page's anchors while the scan is on it.
#[derive(Default)]
struct Pending {
    /// Target and the link's position weight (1 when weighting is off).
    followed: Vec<(u32, f32)>,
    internal: Vec<u32>,
    external: Vec<i64>,
}

/// Reads the graph of FR-002 and FR-003: nodes are internal URLs; edges are
/// distinct followed body anchors between them, except that a redirecting
/// URL has one edge, to its redirect target (none when the chain loops or
/// leaves the site). Two sequential scans: `urls` by id, `links` by source.
/// With `weighting`, `link_positions` must be built first.
pub fn read_graph(
    conn: &Connection,
    weighting: Option<&PositionWeights>,
    watch: &mut Watch<'_>,
) -> rusqlite::Result<Graph> {
    let max_id: i64 = conn.query_row("SELECT COALESCE(MAX(id), 0) FROM urls", [], |r| r.get(0))?;
    let slots = usize::try_from(max_id).unwrap_or(0) + 1;
    let mut index = vec![NONE; slots];
    let mut external = vec![false; slots];
    let mut ids = Vec::new();
    let mut redirect_to = Vec::new();
    {
        let mut stmt = conn.prepare("SELECT id, internal, redirect_to FROM urls ORDER BY id")?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let id: i64 = r.get(0)?;
            if r.get::<_, bool>(1)? {
                index[id as usize] = ids.len() as u32;
                ids.push(id);
                redirect_to.push(r.get::<_, Option<i64>>(2)?);
            } else {
                external[id as usize] = true;
            }
        }
    }
    let node = |id: i64| index.get(id as usize).copied().unwrap_or(NONE);
    let redirect: Vec<u32> = (0..ids.len())
        .map(|i| match redirect_to[i] {
            Some(_) => redirect_target(i, &redirect_to, &node),
            None => NONE,
        })
        .collect();
    let n = ids.len();
    let mut g = Graph {
        offsets: Vec::with_capacity(n + 1),
        followed_in: vec![0; n],
        unique_in: vec![0; n],
        all_in: vec![0; n],
        unique_out: vec![0; n],
        external_out: vec![0; n],
        ..Graph::default()
    };
    g.offsets.push(0);
    let weighted = weighting.is_some();
    if weighted {
        g.out_weight = vec![0.0; n];
    }
    let finish = |g: &mut Graph, i: usize, p: &mut Pending| {
        p.followed.sort_unstable_by_key(|&(t, _)| t);
        p.followed.dedup_by(|later, kept| {
            let same = later.0 == kept.0;
            if same {
                kept.1 = kept.1.max(later.1);
            }
            same
        });
        p.internal.sort_unstable();
        p.internal.dedup();
        p.external.sort_unstable();
        p.external.dedup();
        for &t in &p.internal {
            g.unique_in[t as usize] += 1;
        }
        g.unique_out[i] = p.internal.len() as u32;
        g.external_out[i] = p.external.len() as u32;
        if redirect_to[i].is_some() {
            if redirect[i] != NONE {
                g.targets.push(redirect[i]);
                if weighted {
                    g.weights.push(1.0);
                    g.out_weight[i] = 1.0;
                }
            }
        } else {
            for &(t, w) in &p.followed {
                g.followed_in[t as usize] += 1;
                g.targets.push(t);
                if weighted {
                    g.weights.push(w);
                    g.out_weight[i] += f64::from(w);
                }
            }
        }
        p.followed.clear();
        p.internal.clear();
        p.external.clear();
        g.offsets.push(g.targets.len() as u32);
    };
    let mut pending = Pending::default();
    let mut next = 0usize;
    let mut reported = 0usize;
    let mut stmt = conn.prepare(if weighted {
        "SELECT l.from_id, l.to_id, l.nofollow OR l.page_nofollow OR l.sponsored OR l.ugc,
                COALESCE(p.position, 0)
         FROM links l LEFT JOIN link_positions p ON p.from_id = l.from_id AND p.pos = l.pos
         WHERE l.kind = 'anchor' AND l.to_id IS NOT NULL ORDER BY l.from_id"
    } else {
        "SELECT from_id, to_id, nofollow OR page_nofollow OR sponsored OR ugc, 0 FROM links
         WHERE kind = 'anchor' AND to_id IS NOT NULL ORDER BY from_id"
    })?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        let from = node(r.get(0)?);
        if from == NONE {
            continue;
        }
        let to_id: i64 = r.get(1)?;
        let from = from as usize;
        if from >= reported + 4096 {
            reported = from;
            watch.tick("link_score", from as u64, n as u64)?;
        }
        while next < from {
            finish(&mut g, next, &mut pending);
            next += 1;
        }
        let to = node(to_id);
        if to == NONE {
            if external.get(to_id as usize).copied().unwrap_or(false) {
                pending.external.push(to_id);
            }
        } else if to as usize != from {
            g.all_in[to as usize] += 1;
            pending.internal.push(to);
            if !r.get::<_, bool>(2)? {
                let w = match weighting {
                    Some(w) => w.for_code(r.get(3)?),
                    None => 1.0,
                };
                pending.followed.push((to, w));
            }
        }
    }
    while next < n {
        finish(&mut g, next, &mut pending);
        next += 1;
    }
    g.ids = ids;
    Ok(g)
}

/// Where a redirect chain starting at node `i` ends, within the site: the
/// first node that does not redirect, or `NONE` for a loop, an external
/// target or a chain longer than any crawl allows.
fn redirect_target(i: usize, redirect_to: &[Option<i64>], node: &impl Fn(i64) -> u32) -> u32 {
    let mut cur = i;
    for _ in 0..100 {
        let Some(to) = redirect_to[cur] else {
            return cur as u32;
        };
        let t = node(to);
        if t == NONE || t as usize == i {
            return NONE;
        }
        cur = t as usize;
    }
    NONE
}

/// Damped power iteration (plan D3). Returns the raw ranks (summing to 1)
/// and the number of rounds.
pub fn rank(g: &Graph) -> (Vec<f64>, u32) {
    let n = g.len();
    if n == 0 {
        return (Vec::new(), 0);
    }
    let nf = n as f64;
    let mut r = vec![1.0 / nf; n];
    let mut next = vec![0.0; n];
    let mut rounds = 0;
    while rounds < MAX_ROUNDS {
        rounds += 1;
        let mut dangling = 0.0;
        for (i, ri) in r.iter().enumerate() {
            if g.dangling(i) {
                dangling += ri;
            }
        }
        let base = (1.0 - DAMPING) / nf + DAMPING * dangling / nf;
        next.fill(base);
        for (i, ri) in r.iter().enumerate() {
            if g.dangling(i) {
                continue;
            }
            let out = g.out(i);
            if g.weights.is_empty() {
                let share = DAMPING * ri / out.len() as f64;
                for &t in out {
                    next[t as usize] += share;
                }
            } else {
                let unit = DAMPING * ri / g.out_weight[i];
                let w = &g.weights[g.offsets[i] as usize..g.offsets[i + 1] as usize];
                for (&t, &w) in out.iter().zip(w) {
                    next[t as usize] += unit * f64::from(w);
                }
            }
        }
        let mut change = 0.0;
        for (a, b) in r.iter().zip(&next) {
            change += (a - b).abs();
        }
        std::mem::swap(&mut r, &mut next);
        if change < TOLERANCE {
            break;
        }
    }
    (r, rounds)
}

/// Maps raw ranks to 0-100 on a log curve (FR-004): a URL nothing links to
/// scores 0, the strongest URL 100. When every URL ranks the same (one URL,
/// or no links at all), all score 100.
pub fn scale(g: &Graph, r: &[f64]) -> Vec<f64> {
    let n = r.len();
    if n == 0 {
        return Vec::new();
    }
    // The rank a node with no inlinks ends with: the teleport share plus the
    // spread dangling mass of the last round.
    let nf = n as f64;
    let dangling: f64 = (0..n).filter(|&i| g.dangling(i)).map(|i| r[i]).sum();
    let floor = (1.0 - DAMPING) / nf + DAMPING * dangling / nf;
    let max = r.iter().copied().fold(f64::MIN, f64::max);
    let span = (max / floor).ln();
    r.iter()
        .map(|&x| {
            if span <= 1e-12 {
                100.0
            } else {
                (100.0 * (x / floor).ln() / span).clamp(0.0, 100.0)
            }
        })
        .collect()
}

/// Competition ranks (1, 2, 2, 4) on the value users see: the score at one
/// decimal place. Ties keep the order of discovery.
pub fn ranks(scores: &[f64]) -> Vec<u32> {
    let shown: Vec<i64> = scores.iter().map(|s| (s * 10.0).round() as i64).collect();
    let mut order: Vec<usize> = (0..scores.len()).collect();
    order.sort_by(|&a, &b| shown[b].cmp(&shown[a]).then(a.cmp(&b)));
    let mut out = vec![0; scores.len()];
    for (pos, &i) in order.iter().enumerate() {
        out[i] = if pos > 0 && shown[order[pos - 1]] == shown[i] {
            out[order[pos - 1]]
        } else {
            pos as u32 + 1
        };
    }
    out
}

/// Builds `temp.link_metrics` (data model) from the crawl on `conn`.
pub fn build(
    conn: &Connection,
    weighting: Option<&PositionWeights>,
    watch: &mut Watch<'_>,
) -> rusqlite::Result<Stats> {
    let t0 = std::time::Instant::now();
    let lap = |l: &str| {
        if std::env::var_os("SPIDERLENS_TIMING").is_some() {
            eprintln!(
                "{:>8.0} ms  link_metrics: {l}",
                t0.elapsed().as_secs_f64() * 1000.0
            );
        }
    };
    let g = read_graph(conn, weighting, watch)?;
    lap("graph");
    watch.tick("link_score", g.len() as u64, g.len() as u64)?;
    let (r, rounds) = rank(&g);
    let scores = scale(&g, &r);
    let rank = ranks(&scores);
    lap("rank");
    conn.execute_batch(
        "DROP TABLE IF EXISTS temp.link_metrics;
         CREATE TEMP TABLE link_metrics (
           url_id INTEGER PRIMARY KEY, score REAL NOT NULL, rank INTEGER NOT NULL,
           unique_inlinks INTEGER NOT NULL, followed_inlinks INTEGER NOT NULL, all_inlinks INTEGER NOT NULL,
           unique_outlinks INTEGER NOT NULL, external_outlinks INTEGER NOT NULL, pct_linking REAL NOT NULL);",
    )?;
    let pages: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM urls u WHERE {}", crate::tabs::HTML_OK),
        [],
        |r| r.get(0),
    )?;
    Watch::savepoint(conn, "metrics", || {
        let mut ins =
            conn.prepare("INSERT INTO temp.link_metrics VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)")?;
        for (i, &id) in g.ids.iter().enumerate() {
            let pct = if pages > 0 {
                f64::from(g.unique_in[i]) * 100.0 / pages as f64
            } else {
                0.0
            };
            ins.execute(params![
                id,
                scores[i],
                rank[i],
                g.unique_in[i],
                g.followed_in[i],
                g.all_in[i],
                g.unique_out[i],
                g.external_out[i],
                pct
            ])?;
        }
        Ok(())
    })?;
    lap("written");
    Ok(Stats {
        urls: g.len() as u64,
        edges: g.targets.len() as u64,
        rounds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A graph from edge lists by node index (ids are index + 1).
    fn graph(edges: &[&[u32]]) -> Graph {
        let mut g = Graph {
            ids: (1..=edges.len() as i64).collect(),
            offsets: vec![0],
            followed_in: vec![0; edges.len()],
            ..Graph::default()
        };
        for out in edges {
            g.targets.extend_from_slice(out);
            for &t in *out {
                g.followed_in[t as usize] += 1;
            }
            g.offsets.push(g.targets.len() as u32);
        }
        g
    }

    /// The textbook definition, written the slow way, as the reference for
    /// SC-001.
    fn reference(edges: &[&[u32]]) -> Vec<f64> {
        let n = edges.len();
        let mut r = vec![1.0 / n as f64; n];
        for _ in 0..1000 {
            let mut next = vec![(1.0 - DAMPING) / n as f64; n];
            for (i, out) in edges.iter().enumerate() {
                if out.is_empty() {
                    for v in &mut next {
                        *v += DAMPING * r[i] / n as f64;
                    }
                } else {
                    for &t in *out {
                        next[t as usize] += DAMPING * r[i] / out.len() as f64;
                    }
                }
            }
            r = next;
        }
        r
    }

    /// `graph` with a weight per edge.
    fn weighted(edges: &[&[(u32, f32)]]) -> Graph {
        let plain: Vec<Vec<u32>> = edges.iter().map(|e| e.iter().map(|x| x.0).collect()).collect();
        let refs: Vec<&[u32]> = plain.iter().map(Vec::as_slice).collect();
        let mut g = graph(&refs);
        for e in edges {
            g.weights.extend(e.iter().map(|x| x.1));
            g.out_weight.push(e.iter().map(|x| f64::from(x.1)).sum());
        }
        g
    }

    #[test]
    fn weights_of_one_change_nothing() {
        let edges: &[&[u32]] = &[&[1, 2, 3], &[0, 2], &[0], &[], &[0, 1, 2, 3], &[4]];
        let w: Vec<Vec<(u32, f32)>> = edges
            .iter()
            .map(|e| e.iter().map(|&t| (t, 1.0)).collect())
            .collect();
        let w: Vec<&[(u32, f32)]> = w.iter().map(Vec::as_slice).collect();
        let (a, _) = rank(&graph(edges));
        let (b, _) = rank(&weighted(&w));
        for (a, b) in a.iter().zip(&b) {
            assert!((a - b).abs() < 1e-12);
        }
    }

    #[test]
    fn footer_weight_lowers_a_footer_only_page() {
        // 0 links to 1 from content and to 2 from the footer.
        let even = weighted(&[&[(1, 1.0), (2, 1.0)], &[(0, 1.0)], &[(0, 1.0)]]);
        let footer = weighted(&[&[(1, 1.0), (2, 0.2)], &[(0, 1.0)], &[(0, 1.0)]]);
        let (e, _) = rank(&even);
        let (f, _) = rank(&footer);
        assert!((e[1] - e[2]).abs() < 1e-12);
        assert!(f[2] < e[2] && f[1] > e[1]);
        assert!((f.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        // A page whose links all weigh 0 passes nothing: it is dangling.
        let zero = weighted(&[&[(1, 0.0)], &[(0, 1.0)]]);
        assert!(zero.dangling(0));
    }

    #[test]
    fn matches_the_reference() {
        let edges: &[&[u32]] = &[&[1, 2, 3], &[0, 2], &[0], &[], &[0, 1, 2, 3], &[4]];
        let (r, rounds) = rank(&graph(edges));
        assert!(rounds < MAX_ROUNDS);
        for (a, b) in r.iter().zip(reference(edges)) {
            assert!((a - b).abs() < 1e-9, "{a} vs {b}");
        }
        assert!((r.iter().sum::<f64>() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn hub_scores_100_and_unlinked_pages_0() {
        // 0 is linked from everyone; 3 from nobody.
        let edges: &[&[u32]] = &[&[1, 2], &[0], &[0], &[0, 1]];
        let g = graph(edges);
        let (r, _) = rank(&g);
        let s = scale(&g, &r);
        assert!((s[0] - 100.0).abs() < 1e-9);
        assert!(s[3].abs() < 1e-9);
        assert!(s[1] > s[2] && s[2] > s[3]);
        assert_eq!(ranks(&s), vec![1, 2, 3, 4]);
    }

    #[test]
    fn one_page_or_no_links_score_100() {
        let g = graph(&[&[]]);
        assert_eq!(scale(&g, &rank(&g).0), vec![100.0]);
        let g = graph(&[&[], &[], &[]]);
        assert_eq!(scale(&g, &rank(&g).0), vec![100.0; 3]);
        assert_eq!(ranks(&[100.0; 3]), vec![1, 1, 1]);
    }

    #[test]
    fn ties_share_a_rank() {
        assert_eq!(ranks(&[50.0, 80.04, 80.0, 10.0]), vec![3, 1, 1, 4]);
    }

    #[test]
    fn deterministic_bits() {
        // FR-005, SC-008: the same graph gives the same bits; the hash is the
        // same on every platform CI runs on.
        let edges: Vec<Vec<u32>> = (0..500u32)
            .map(|i| {
                (1..=(i % 7))
                    .map(|k| (i * 31 + k * 17) % 500)
                    .filter(|&t| t != i)
                    .collect()
            })
            .collect();
        let refs: Vec<&[u32]> = edges.iter().map(Vec::as_slice).collect();
        let g = graph(&refs);
        let (r, _) = rank(&g);
        let s = scale(&g, &r);
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        // Ranks are plain arithmetic, so their bits are compared. Scores go
        // through `ln`, whose last bit may differ between system maths
        // libraries, so they are compared as shown (one decimal).
        let shown = s.iter().map(|v| ((v * 10.0).round() as i64).to_le_bytes());
        for bytes in r.iter().map(|v| v.to_bits().to_le_bytes()).chain(shown) {
            for b in bytes {
                h = (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
            }
        }
        let again = rank(&g).0;
        assert_eq!(r, again);
        assert_eq!(
            h, EXPECTED_HASH,
            "scores changed: update EXPECTED_HASH only for an intended method change"
        );
    }

    const EXPECTED_HASH: u64 = 4_231_180_848_031_882_861;
}
