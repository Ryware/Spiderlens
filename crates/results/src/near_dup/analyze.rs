//! The near duplicate computation over signatures (spec 020, plan D4):
//! banded LSH for candidates, signature comparison for scores, per-page
//! summaries and union-find clusters. Pure: items in, results out.

use std::sync::atomic::{AtomicBool, Ordering};

use spiderlens_engine::extract::shingle::{BINS, Signature, seeded_hash, similarity_of};

/// One compared page.
#[derive(Debug, Clone)]
pub struct Item {
    pub id: i64,
    pub sig: Signature,
    /// Main content hash: equal = exact content duplicate.
    pub hash: i64,
    pub inlinks: i64,
    pub url_len: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct Params {
    pub threshold: u8,
    pub floor: u8,
    /// Matches kept per page (FR-013).
    pub top_k: usize,
    /// Buckets larger than this become dense groups (research R4).
    pub dense_bucket: usize,
    /// Dense groups up to this size are compared all-pairs (4,000 members
    /// is 1.6 x 10^7 comparisons; 50,000 cost 4.9 s at 20,000 pages).
    pub exact_limit: usize,
    /// Members compared per page in larger groups (at most the group size).
    pub sample: usize,
}

impl Params {
    pub fn new(threshold: u8, floor: u8) -> Self {
        Self {
            threshold,
            floor: floor.min(threshold),
            top_k: 50,
            dense_bucket: 256,
            exact_limit: 4_000,
            sample: 1_000,
        }
    }

    /// Fewest equal bins among the first [`PREFIX`] for a pair that could
    /// still reach the floor: the floor's expected share less a 0.2 margin,
    /// over five standard deviations of the prefix estimate (a true pair at
    /// the floor is ruled out about once in 10^8).
    fn prefix_gate(&self) -> u32 {
        let j = f64::from(self.floor) / 100.0;
        let p = j + (1.0 - j) / 256.0;
        ((p - 0.2).max(0.0) * PREFIX as f64).floor() as u32
    }
}

/// Bins compared before the full signature (research R4): unrelated pages
/// that share a bucket (thin templated pages do) cost an eighth.
const PREFIX: usize = 128;

/// Rows of a dense group scored together against each compared signature.
const TILE: usize = 32;

/// Similarity of `a` and `b`, or `None` when the prefix rules the pair out.
fn score(a: &Item, b: &Item, gate: u32) -> Option<u8> {
    if a.sig.equal_prefix(&b.sig, PREFIX) < gate {
        return None;
    }
    Some(similarity_of(a.sig.equal_bins(&b.sig)))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageResult {
    /// Index into the items and similarity, when at least the floor.
    pub closest: Option<(usize, u8)>,
    /// Other pages at or above the threshold.
    pub near_count: u32,
    pub count_is_min: bool,
    /// Index into [`Analysis::clusters`].
    pub cluster: Option<usize>,
    /// Best matches at or above the floor, highest first: (other, sim).
    pub top: Vec<(usize, u8)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cluster {
    pub size: usize,
    pub min_sim: u8,
    pub max_sim: u8,
    pub representative: usize,
    pub exact: bool,
}

#[derive(Debug, Default)]
pub struct Analysis {
    pub pages: Vec<PageResult>,
    /// Numbered by position: cluster 1 is `clusters[0]`.
    pub clusters: Vec<Cluster>,
}

#[derive(Debug, thiserror::Error)]
#[error("analysis cancelled")]
pub struct Cancelled;

/// Rows per band and number of bands for a floor (research R3): the most
/// rows that still make a pair at the floor a candidate with probability
/// at least 0.999. One row matches by chance once in 256 (8-bit bins).
pub fn bands(floor: u8) -> (usize, usize) {
    let j = f64::from(floor.clamp(1, 100)) / 100.0;
    let p = j + (1.0 - j) / 256.0;
    let mut best = (1, BINS);
    for r in 1..=32 {
        let b = BINS / r;
        if 1.0 - (1.0 - p.powi(r as i32)).powi(b as i32) >= 0.999 {
            best = (r, b);
        }
    }
    best
}

struct Dsu(Vec<u32>);

impl Dsu {
    fn new(n: usize) -> Self {
        Self((0..n as u32).collect())
    }

    fn find(&mut self, mut x: u32) -> u32 {
        while self.0[x as usize] != x {
            let up = self.0[self.0[x as usize] as usize];
            self.0[x as usize] = up;
            x = up;
        }
        x
    }

    fn union(&mut self, a: u32, b: u32) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            // Smaller index as root keeps results independent of order.
            let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
            self.0[hi as usize] = lo;
        }
    }
}

/// Per-page accumulator.
#[derive(Debug, Clone, Default)]
struct Stats {
    near: u32,
    is_min: bool,
    top: Vec<(u8, u32)>,
    /// Lowest and highest similarity of this page's pairs at or above the
    /// threshold.
    lo: u8,
    hi: u8,
}

impl Stats {
    fn add(&mut self, other: u32, sim: u8, p: &Params) {
        if sim >= p.threshold {
            if self.near == 0 {
                self.lo = sim;
                self.hi = sim;
            } else {
                self.lo = self.lo.min(sim);
                self.hi = self.hi.max(sim);
            }
            self.near += 1;
        }
        self.top.push((sim, other));
        if self.top.len() >= p.top_k * 4 {
            self.trim(p.top_k);
        }
    }

    fn trim(&mut self, k: usize) {
        // Highest similarity first, then lowest index, so ties are stable.
        self.top
            .sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        self.top.truncate(k);
    }

    fn merge(&mut self, other: Stats, k: usize) {
        if other.near > 0 {
            if self.near == 0 {
                (self.lo, self.hi) = (other.lo, other.hi);
            } else {
                self.lo = self.lo.min(other.lo);
                self.hi = self.hi.max(other.hi);
            }
        }
        self.near += other.near;
        self.is_min |= other.is_min;
        self.top.extend(other.top);
        self.trim(k);
    }
}

fn band_key(sig: &Signature, band: usize, rows: usize) -> u64 {
    seeded_hash(&sig.0[band * rows..band * rows + rows], band as u64)
}

/// Runs the analysis. `progress(done, total)` is called between steps.
pub fn analyze(
    items: &[Item],
    p: &Params,
    cancel: &AtomicBool,
    mut progress: impl FnMut(usize, usize),
) -> Result<Analysis, Cancelled> {
    let n = items.len();
    let check = || {
        if cancel.load(Ordering::Relaxed) {
            Err(Cancelled)
        } else {
            Ok(())
        }
    };
    let (rows, nbands) = bands(p.floor);
    let total = nbands + 2;
    let mut dense = Dsu::new(n);
    let mut in_dense = vec![false; n];
    let mut candidates: Vec<u64> = Vec::new();
    let mut dedup_at = 4 * n + 1_000_000;
    let mut keys: Vec<(u64, u32)> = Vec::with_capacity(n);
    for band in 0..nbands {
        check()?;
        keys.clear();
        keys.extend(
            items
                .iter()
                .enumerate()
                .map(|(i, it)| (band_key(&it.sig, band, rows), i as u32)),
        );
        keys.sort_unstable();
        let mut start = 0;
        while start < keys.len() {
            let mut end = start + 1;
            while end < keys.len() && keys[end].0 == keys[start].0 {
                end += 1;
            }
            let bucket = &keys[start..end];
            if bucket.len() > p.dense_bucket {
                for &(_, m) in bucket {
                    dense.union(bucket[0].1, m);
                    in_dense[m as usize] = true;
                }
            } else {
                for (x, &(_, a)) in bucket.iter().enumerate() {
                    for &(_, b) in &bucket[x + 1..] {
                        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
                        candidates.push((u64::from(lo) << 32) | u64::from(hi));
                    }
                }
            }
            start = end;
        }
        // Keep the candidate list from growing with repeats across bands;
        // the limit doubles with what survives, so sorting stays amortised.
        if candidates.len() > dedup_at {
            candidates.sort_unstable();
            candidates.dedup();
            dedup_at = dedup_at.max(2 * candidates.len());
        }
        progress(band + 1, total);
    }
    candidates.sort_unstable();
    candidates.dedup();

    let mut stats = vec![Stats::default(); n];
    let mut clusters = Dsu::new(n);
    let dense_root: Vec<Option<u32>> = (0..n)
        .map(|i| in_dense[i].then(|| dense.find(i as u32)))
        .collect();
    let gate = p.prefix_gate();
    for &c in &candidates {
        let (a, b) = ((c >> 32) as u32, c as u32);
        if let (Some(ra), Some(rb)) = (dense_root[a as usize], dense_root[b as usize])
            && ra == rb
        {
            continue; // scored with its dense group
        }
        let Some(sim) = score(&items[a as usize], &items[b as usize], gate) else {
            continue;
        };
        if sim >= p.floor {
            stats[a as usize].add(b, sim, p);
            stats[b as usize].add(a, sim, p);
            if sim >= p.threshold {
                clusters.union(a, b);
            }
        }
    }
    check()?;
    progress(nbands + 1, total);

    // Dense groups: all pairs once, rows split across threads.
    let mut groups: std::collections::BTreeMap<u32, Vec<u32>> = std::collections::BTreeMap::new();
    for (i, r) in dense_root.iter().enumerate() {
        if let Some(r) = r {
            groups.entry(*r).or_default().push(i as u32);
        }
    }
    for members in groups.values().filter(|m| m.len() > 1) {
        let (results, edges) = score_group(items, members, p, cancel)?;
        for (i, s) in results {
            stats[i as usize].merge(s, p.top_k);
        }
        for (a, b) in edges {
            clusters.union(a, b);
        }
    }
    check()?;

    // Pages and clusters.
    let mut pages: Vec<PageResult> = stats
        .into_iter()
        .map(|mut s| {
            s.trim(p.top_k);
            PageResult {
                closest: s.top.first().map(|&(sim, o)| (o as usize, sim)),
                near_count: s.near,
                count_is_min: s.is_min,
                cluster: None,
                top: s.top.iter().map(|&(sim, o)| (o as usize, sim)).collect(),
            }
        })
        .collect();
    let mut members: std::collections::BTreeMap<u32, Vec<usize>> = std::collections::BTreeMap::new();
    for (i, page) in pages.iter().enumerate() {
        if page.near_count > 0 {
            members.entry(clusters.find(i as u32)).or_default().push(i);
        }
    }
    let mut list: Vec<Vec<usize>> = members.into_values().filter(|m| m.len() > 1).collect();
    list.sort_by(|a, b| b.len().cmp(&a.len()).then(items[a[0]].id.cmp(&items[b[0]].id)));
    let mut out = Vec::with_capacity(list.len());
    for (number, m) in list.iter().enumerate() {
        let mut lo = 100u8;
        let mut hi = 0u8;
        for &i in m {
            pages[i].cluster = Some(number);
        }
        let mut stats_lohi = |i: usize| {
            for &(o, sim) in &pages[i].top {
                if sim >= p.threshold && pages[o].cluster == Some(number) {
                    lo = lo.min(sim);
                    hi = hi.max(sim);
                }
            }
        };
        for &i in m {
            stats_lohi(i);
        }
        let representative = *m
            .iter()
            .max_by(|&&a, &&b| {
                items[a]
                    .inlinks
                    .cmp(&items[b].inlinks)
                    .then(items[b].url_len.cmp(&items[a].url_len))
                    .then(items[b].id.cmp(&items[a].id))
            })
            .expect("clusters have members");
        let exact = m.iter().all(|&i| items[i].hash == items[m[0]].hash);
        out.push(Cluster {
            size: m.len(),
            min_sim: lo.min(hi),
            max_sim: hi,
            representative,
            exact,
        });
    }
    progress(total, total);
    Ok(Analysis { pages, clusters: out })
}

/// Per-member stats and spanning edges of one dense group.
type GroupScore = (Vec<(u32, Stats)>, Vec<(u32, u32)>);

/// Scores a dense group. Returns per-member stats and edges at or above the
/// threshold that connect the same members the full threshold graph does
/// (a spanning set from per-thread union-finds, never every edge).
fn score_group(
    items: &[Item],
    members: &[u32],
    p: &Params,
    cancel: &AtomicBool,
) -> Result<GroupScore, Cancelled> {
    let sampled = members.len() > p.exact_limit;
    let gate = p.prefix_gate();
    // Positions in `members` to compare each member with.
    let compare: Vec<usize> = if sampled {
        // Distinct, in-range positions: step >= 1 and k < n, so k * step < len.
        let n = p.sample.min(members.len()).max(1);
        let step = members.len() as f64 / n as f64;
        (0..n).map(|k| (k as f64 * step) as usize).collect()
    } else {
        (0..members.len()).collect()
    };
    let targets: Vec<Signature> = compare
        .iter()
        .map(|&b| items[members[b] as usize].sig.clone())
        .collect();
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(members.len());
    let chunk = members.len().div_ceil(threads);
    type Part = Result<(Vec<(u32, Stats)>, Vec<(u32, u32)>), Cancelled>;
    let parts: Vec<Part> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..members.len())
            .step_by(chunk)
            .map(|first| {
                let (compare, targets) = (&compare, &targets);
                scope.spawn(move || {
                    let rows = first..(first + chunk).min(members.len());
                    let mut local = Vec::with_capacity(rows.len());
                    let mut dsu = Dsu::new(members.len());
                    // Rows in tiles that stay in cache while the compared
                    // signatures stream past (contiguous, in `targets`).
                    let rows: Vec<usize> = rows.collect();
                    for tile in rows.chunks(TILE) {
                        if cancel.load(Ordering::Relaxed) {
                            return Err(Cancelled);
                        }
                        let sigs: Vec<&Signature> =
                            tile.iter().map(|&a| &items[members[a] as usize].sig).collect();
                        let mut stats: Vec<Stats> = tile
                            .iter()
                            .map(|_| Stats {
                                is_min: sampled,
                                ..Stats::default()
                            })
                            .collect();
                        for (&b, target) in compare.iter().zip(targets) {
                            for (t, &a) in tile.iter().enumerate() {
                                if b == a || sigs[t].equal_prefix(target, PREFIX) < gate {
                                    continue;
                                }
                                let sim = similarity_of(sigs[t].equal_bins(target));
                                if sim >= p.floor {
                                    stats[t].add(members[b], sim, p);
                                    if sim >= p.threshold {
                                        dsu.union(a as u32, b as u32);
                                    }
                                }
                            }
                        }
                        for (&a, mut st) in tile.iter().zip(stats) {
                            st.trim(p.top_k);
                            local.push((members[a], st));
                        }
                    }
                    let edges: Vec<(u32, u32)> = (0..members.len() as u32)
                        .filter_map(|x| {
                            let r = dsu.find(x);
                            (r != x).then(|| (members[x as usize], members[r as usize]))
                        })
                        .collect();
                    Ok((local, edges))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("scoring thread"))
            .collect()
    });
    let mut results = Vec::with_capacity(members.len());
    let mut edges = Vec::new();
    for part in parts {
        let (r, e) = part?;
        results.extend(r);
        edges.extend(e);
    }
    Ok((results, edges))
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiderlens_engine::extract::shingle::signature;

    fn text(seed: usize, n: usize) -> Vec<String> {
        (0..n).map(|i| format!("s{seed}w{i}")).collect()
    }

    /// `base` with `change` words replaced, spread out.
    fn variant(base: &[String], change: usize, tag: &str) -> String {
        let mut v = base.to_vec();
        for k in 0..change {
            let at = (k * 7919 + 13) % v.len();
            v[at] = format!("{tag}{k}");
        }
        v.join(" ")
    }

    fn item(id: i64, t: &str) -> Item {
        Item {
            id,
            sig: signature(t).unwrap(),
            hash: spiderlens_engine::extract::text::hash(t),
            inlinks: 0,
            url_len: 10,
        }
    }

    fn run(items: &[Item], p: Params) -> Analysis {
        analyze(items, &p, &AtomicBool::new(false), |_, _| {}).unwrap()
    }

    #[test]
    fn band_table_matches_research() {
        assert!(bands(80).0 * bands(80).1 <= BINS);
        // Higher floors use more rows per band (fewer chance candidates).
        assert!(bands(90).0 > bands(80).0 && bands(80).0 > bands(50).0);
        for f in [50u8, 60, 70, 80, 85, 90, 95] {
            let (r, b) = bands(f);
            let j = f64::from(f) / 100.0;
            let p = j + (1.0 - j) / 256.0;
            assert!(
                1.0 - (1.0 - p.powi(r as i32)).powi(b as i32) >= 0.999,
                "floor {f}"
            );
        }
    }

    #[test]
    fn groups_pairs_and_clusters() {
        let base = text(1, 600);
        let other = text(2, 600);
        let items = vec![
            item(1, &base.join(" ")),
            item(2, &base.join(" ")),          // exact copy
            item(3, &variant(&base, 3, "x")),  // ~97%
            item(4, &variant(&base, 60, "y")), // well under 90
            item(5, &other.join(" ")),         // unrelated
            item(6, &variant(&other, 2, "z")), // near 5
        ];
        let a = run(&items, Params::new(90, 80));
        assert_eq!(a.pages[0].near_count, 2);
        assert_eq!(a.pages[0].closest.map(|c| c.0), Some(1));
        assert_eq!(a.pages[0].closest.map(|c| c.1), Some(100));
        assert_eq!(a.pages[3].near_count, 0);
        assert_eq!(a.pages[4].near_count, 1);
        assert_eq!(a.clusters.len(), 2);
        assert_eq!(a.clusters[0].size, 3);
        assert_eq!(a.pages[0].cluster, Some(0));
        assert_eq!(a.pages[5].cluster, Some(1));
        assert_eq!(a.pages[3].cluster, None);
        assert!(!a.clusters[0].exact);
        assert!(a.clusters[0].max_sim == 100 && a.clusters[0].min_sim >= 90);
    }

    #[test]
    fn lower_threshold_adds_pairs() {
        // US1.4 and US1.5: an 85% pair is under 90 but shown as closest
        // match, and joins at 80.
        let base = text(3, 800);
        let mut items = vec![item(1, &base.join(" "))];
        // Find a change count that lands near 85%.
        let mut t = String::new();
        for change in 10..80 {
            t = variant(&base, change, "q");
            let s = signature(&t).unwrap().similarity(&items[0].sig);
            if (83..=87).contains(&s) {
                break;
            }
        }
        items.push(item(2, &t));
        let at90 = run(&items, Params::new(90, 80));
        assert_eq!(at90.pages[1].near_count, 0);
        assert!(at90.pages[1].closest.is_some_and(|(_, s)| (83..=87).contains(&s)));
        let at80 = run(&items, Params::new(80, 80));
        assert_eq!(at80.pages[1].near_count, 1);
        assert_eq!(at80.clusters.len(), 1);
    }

    #[test]
    fn dense_groups_count_exactly_and_sample_above_the_limit() {
        // 400 near-identical template pages: buckets above 256 become a
        // dense group; every page counts the 399 others.
        let base = text(4, 300);
        let items: Vec<Item> = (0..400)
            .map(|i| item(i, &variant(&base, 1, &format!("t{i}_"))))
            .collect();
        let a = run(&items, Params::new(90, 80));
        assert!(a.pages.iter().all(|p| p.near_count == 399 && !p.count_is_min));
        assert!(a.pages.iter().all(|p| p.top.len() == 50));
        assert_eq!(a.clusters.len(), 1);
        assert_eq!(a.clusters[0].size, 400);

        let mut p = Params::new(90, 80);
        p.exact_limit = 100;
        p.sample = 50;
        let s = run(&items, p);
        assert!(s.pages.iter().all(|p| p.count_is_min && p.near_count >= 49));
        assert_eq!(s.clusters[0].size, 400);
    }

    #[test]
    fn representative_and_numbering_are_stable() {
        let a = text(5, 300);
        let b = text(6, 300);
        let mut items = vec![
            item(10, &a.join(" ")),
            item(11, &variant(&a, 1, "m")),
            item(20, &b.join(" ")),
            item(21, &variant(&b, 1, "n")),
            item(22, &variant(&b, 2, "o")),
        ];
        items[1].inlinks = 5;
        let r = run(&items, Params::new(90, 80));
        assert_eq!(r.clusters[0].size, 3, "largest first");
        assert_eq!(r.clusters[1].representative, 1, "most inlinks");
    }

    #[test]
    fn cancel_stops() {
        let items = vec![item(1, "a b c d e f"), item(2, "a b c d e g")];
        let stop = AtomicBool::new(true);
        assert!(analyze(&items, &Params::new(90, 80), &stop, |_, _| {}).is_err());
    }
}
