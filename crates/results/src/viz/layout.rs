//! Tree layouts (plan D5), deterministic and O(n).
//!
//! Leaves take consecutive slots in depth-first order and each parent sits
//! midway between its first and last child, so subtrees never overlap and
//! parents are centred over their children. The radial layout maps the
//! same slots to angles and levels to rings, so both show the same nodes.

/// Distance between levels and between leaf slots, in layout units.
pub const LEVEL: f32 = 220.0;
pub const SLOT: f32 = 22.0;
/// Radius of the first ring of the radial layout.
pub const RING: f32 = 160.0;

/// Leaf-slot position of every node (`parents` in preorder, parent first).
fn slots(parents: &[u32]) -> Vec<f32> {
    let n = parents.len();
    let mut first = vec![f32::NAN; n];
    let mut last = vec![f32::NAN; n];
    let mut has_child = vec![false; n];
    for &p in parents.iter().skip(1) {
        if p != u32::MAX {
            has_child[p as usize] = true;
        }
    }
    let mut slot = vec![0.0f32; n];
    let mut next = 0.0f32;
    // Preorder: a leaf takes the next slot when reached.
    for i in 0..n {
        if !has_child[i] {
            slot[i] = next;
            next += 1.0;
        }
    }
    // Reverse preorder visits children before parents.
    for i in (0..n).rev() {
        if has_child[i] {
            slot[i] = (first[i] + last[i]) / 2.0;
        }
        let p = parents[i];
        if p != u32::MAX {
            let p = p as usize;
            // Reverse order: the first seen is the last child.
            if last[p].is_nan() {
                last[p] = slot[i];
            }
            first[p] = slot[i];
        }
    }
    slot
}

/// Left to right: x by level, y by slot.
pub fn tidy(parents: &[u32], levels: &[u32]) -> Vec<(f32, f32)> {
    slots(parents)
        .into_iter()
        .zip(levels)
        .map(|(s, &l)| (l as f32 * LEVEL, s * SLOT))
        .collect()
}

/// Rings by level, angle by slot; the root at the centre.
pub fn radial(parents: &[u32], levels: &[u32]) -> Vec<(f32, f32)> {
    let s = slots(parents);
    let leaves = s.iter().copied().fold(0.0f32, f32::max) + 1.0;
    // Keep neighbouring leaves on the outer ring about one slot apart.
    let deepest = levels.iter().copied().max().unwrap_or(0) as f32;
    let ring = RING.max(leaves * SLOT / std::f32::consts::TAU / deepest.max(1.0));
    s.into_iter()
        .zip(levels)
        .map(|(slot, &l)| {
            if l == 0 {
                return (0.0, 0.0);
            }
            let a = slot / leaves * std::f32::consts::TAU;
            let r = l as f32 * ring;
            (r * a.cos(), r * a.sin())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: u32 = u32::MAX;

    /// root(0) -> a(1) -> [a1(2), a2(3)], b(4) -> [b1(5)], c(6)
    fn sample() -> (Vec<u32>, Vec<u32>) {
        (vec![N, 0, 1, 1, 0, 4, 0], vec![0, 1, 2, 2, 1, 2, 1])
    }

    #[test]
    fn leaves_take_slots_and_parents_centre() {
        let (p, _) = sample();
        let s = slots(&p);
        assert_eq!(s[2], 0.0);
        assert_eq!(s[3], 1.0);
        assert_eq!(s[1], 0.5);
        assert_eq!(s[5], 2.0);
        assert_eq!(s[4], 2.0);
        assert_eq!(s[6], 3.0);
        assert_eq!(s[0], 1.75, "between first (a, 0.5) and last (c, 3) child");
    }

    #[test]
    fn tidy_never_overlaps_at_one_level() {
        // A wide random-ish tree: no two nodes on a level share a y.
        let mut parents = vec![N];
        let mut levels = vec![0];
        for i in 1..400u32 {
            let p = (i * 7919) % i;
            parents.push(p);
            levels.push(levels[p as usize] + 1);
        }
        // Preorder is required: rebuild it.
        let mut kids = vec![Vec::new(); parents.len()];
        for (i, &p) in parents.iter().enumerate().skip(1) {
            kids[p as usize].push(i);
        }
        let (mut pre_p, mut pre_l, mut stack) = (Vec::new(), Vec::new(), vec![(0usize, N)]);
        while let Some((i, p)) = stack.pop() {
            let me = pre_p.len() as u32;
            pre_p.push(p);
            pre_l.push(levels[i]);
            for &c in kids[i].iter().rev() {
                stack.push((c, me));
            }
        }
        let pos = tidy(&pre_p, &pre_l);
        let mut seen = std::collections::HashSet::new();
        for (k, &(x, y)) in pos.iter().enumerate() {
            assert!(seen.insert((x.to_bits(), y.to_bits())), "node {k} overlaps");
        }
    }

    #[test]
    fn radial_has_the_same_nodes_and_rings_by_level() {
        let (p, l) = sample();
        let pos = radial(&p, &l);
        assert_eq!(pos.len(), p.len());
        assert_eq!(pos[0], (0.0, 0.0));
        let d = |(x, y): (f32, f32)| (x * x + y * y).sqrt();
        assert!((d(pos[1]) - d(pos[4])).abs() < 1e-3, "same level, same ring");
        assert!(d(pos[2]) > d(pos[1]));
    }
}
