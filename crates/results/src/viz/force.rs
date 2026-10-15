//! Force-directed layout (plan D5): ForceAtlas2-style forces with a
//! Barnes-Hut approximation of repulsion. One thread, a fixed summation
//! order and seeded starting positions make the result the same on every
//! machine for the same crawl and settings (FR-014).

/// Barnes-Hut opening angle.
const THETA: f64 = 1.0;
const REPULSION: f64 = 2.0;
const GRAVITY: f64 = 0.05;
pub const MAX_ITERATIONS: u32 = 200;

/// A deterministic starting position for a node, from its id.
pub fn seed_position(id: i64) -> (f64, f64) {
    // SplitMix64: well spread, the same everywhere.
    let mut z = (id as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    let a = (z & 0xFFFF_FFFF) as f64 / u32::MAX as f64 * std::f64::consts::TAU;
    let r = ((z >> 32) as f64 / u32::MAX as f64).sqrt() * 100.0;
    (r * a.cos(), r * a.sin())
}

/// Quadtree node: a square cell with the total mass and its centre.
struct Cell {
    cx: f64,
    cy: f64,
    half: f64,
    mass: f64,
    mx: f64,
    my: f64,
    /// Leaf with one body: its index.
    body: Option<usize>,
    children: Option<[usize; 4]>,
}

struct Quad {
    cells: Vec<Cell>,
}

impl Quad {
    fn build(pos: &[(f64, f64)], mass: &[f64]) -> Self {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for &(x, y) in pos {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        let half = ((x1 - x0).max(y1 - y0) / 2.0).max(1.0) + 1.0;
        let mut q = Quad {
            cells: vec![Cell {
                cx: (x0 + x1) / 2.0,
                cy: (y0 + y1) / 2.0,
                half,
                mass: 0.0,
                mx: 0.0,
                my: 0.0,
                body: None,
                children: None,
            }],
        };
        for i in 0..pos.len() {
            q.insert(0, i, pos, mass, 0);
        }
        q
    }

    fn insert(&mut self, c: usize, i: usize, pos: &[(f64, f64)], mass: &[f64], depth: u32) {
        let (x, y) = pos[i];
        let m = mass[i];
        {
            let cell = &mut self.cells[c];
            let total = cell.mass + m;
            cell.mx = (cell.mx * cell.mass + x * m) / total;
            cell.my = (cell.my * cell.mass + y * m) / total;
            cell.mass = total;
        }
        if self.cells[c].children.is_none() {
            match self.cells[c].body {
                None if self.cells[c].mass == m => {
                    self.cells[c].body = Some(i);
                    return;
                }
                // Coincident points or too deep: keep them together.
                _ if depth > 40 => return,
                Some(j) => {
                    self.split(c);
                    self.cells[c].body = None;
                    let k = self.child_for(c, pos[j]);
                    self.insert(k, j, pos, mass, depth + 1);
                }
                None => {}
            }
        }
        if self.cells[c].children.is_none() {
            self.split(c);
        }
        let k = self.child_for(c, (x, y));
        self.insert(k, i, pos, mass, depth + 1);
    }

    fn split(&mut self, c: usize) {
        let (cx, cy, h) = (self.cells[c].cx, self.cells[c].cy, self.cells[c].half / 2.0);
        let base = self.cells.len();
        for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            self.cells.push(Cell {
                cx: cx + dx * h,
                cy: cy + dy * h,
                half: h,
                mass: 0.0,
                mx: 0.0,
                my: 0.0,
                body: None,
                children: None,
            });
        }
        self.cells[c].children = Some([base, base + 1, base + 2, base + 3]);
    }

    fn child_for(&self, c: usize, (x, y): (f64, f64)) -> usize {
        let cell = &self.cells[c];
        let k = usize::from(x >= cell.cx) + 2 * usize::from(y >= cell.cy);
        cell.children.expect("split")[k]
    }

    /// Repulsion on body `i` (mass `m`) from everything in cell `c`.
    fn push(&self, c: usize, i: usize, (x, y): (f64, f64), m: f64, out: &mut (f64, f64)) {
        let cell = &self.cells[c];
        if cell.mass == 0.0 || cell.body == Some(i) {
            return;
        }
        let dx = x - cell.mx;
        let dy = y - cell.my;
        let d2 = dx * dx + dy * dy;
        let far = cell.children.is_none() || (2.0 * cell.half) * (2.0 * cell.half) < THETA * THETA * d2;
        if far {
            if d2 < 1e-9 {
                return;
            }
            // ForceAtlas2: k (m_i)(m_j) / d, along the unit vector.
            let f = REPULSION * m * cell.mass / d2;
            out.0 += dx * f;
            out.1 += dy * f;
        } else if let Some(kids) = cell.children {
            for k in kids {
                self.push(k, i, (x, y), m, out);
            }
        }
    }
}

/// Lays out `n` nodes joined by `edges` from `start` positions. `pinned`
/// nodes do not move. Returns the positions and whether it converged.
pub fn layout(start: Vec<(f64, f64)>, edges: &[(u32, u32)], pinned: &[bool]) -> (Vec<(f64, f64)>, bool) {
    let n = start.len();
    let mut pos = start;
    if n < 2 {
        return (pos, true);
    }
    let mut degree = vec![0u32; n];
    for &(a, b) in edges {
        degree[a as usize] += 1;
        degree[b as usize] += 1;
    }
    let mass: Vec<f64> = degree.iter().map(|&d| f64::from(d) + 1.0).collect();
    let mut settled = false;
    for it in 0..MAX_ITERATIONS {
        let quad = Quad::build(&pos, &mass);
        let mut force = vec![(0.0f64, 0.0f64); n];
        for i in 0..n {
            let mut f = (0.0, 0.0);
            quad.push(0, i, pos[i], mass[i], &mut f);
            // Gravity towards the centre, stronger for hubs.
            let (x, y) = pos[i];
            let d = (x * x + y * y).sqrt().max(1e-9);
            f.0 -= GRAVITY * mass[i] * x / d;
            f.1 -= GRAVITY * mass[i] * y / d;
            force[i] = f;
        }
        for &(a, b) in edges {
            let (a, b) = (a as usize, b as usize);
            let dx = pos[b].0 - pos[a].0;
            let dy = pos[b].1 - pos[a].1;
            // Linear attraction.
            force[a].0 += dx * 0.01;
            force[a].1 += dy * 0.01;
            force[b].0 -= dx * 0.01;
            force[b].1 -= dy * 0.01;
        }
        // Cooling: big steps first, small ones to settle.
        let cool = 1.0 - f64::from(it) / f64::from(MAX_ITERATIONS);
        let max_step = 2.0 + 20.0 * cool;
        let mut moved = 0.0;
        for i in 0..n {
            if pinned[i] {
                continue;
            }
            let (fx, fy) = force[i];
            let len = (fx * fx + fy * fy).sqrt();
            let step = (len / mass[i]).min(max_step);
            if len > 1e-12 {
                pos[i].0 += fx / len * step;
                pos[i].1 += fy / len * step;
                moved += step;
            }
        }
        if it > 50 && moved / (n as f64) < 0.05 {
            settled = true;
            break;
        }
    }
    (pos, settled)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two cliques of `k` joined by one edge.
    fn two_clusters(k: u32) -> Vec<(u32, u32)> {
        let mut e = Vec::new();
        for c in 0..2 {
            for i in 0..k {
                for j in i + 1..k {
                    e.push((c * k + i, c * k + j));
                }
            }
        }
        e.push((0, k));
        e
    }

    fn centroid(p: &[(f64, f64)]) -> (f64, f64) {
        let n = p.len() as f64;
        (
            p.iter().map(|q| q.0).sum::<f64>() / n,
            p.iter().map(|q| q.1).sum::<f64>() / n,
        )
    }

    #[test]
    fn clusters_are_drawn_apart() {
        let k = 12;
        let start: Vec<_> = (0..2 * k).map(|i| seed_position(i64::from(i))).collect();
        let (pos, _) = layout(start, &two_clusters(k), &vec![false; 2 * k as usize]);
        let (a, b) = pos.split_at(k as usize);
        let (ca, cb) = (centroid(a), centroid(b));
        let between = ((ca.0 - cb.0).powi(2) + (ca.1 - cb.1).powi(2)).sqrt();
        let spread = |p: &[(f64, f64)], c: (f64, f64)| {
            p.iter()
                .map(|q| ((q.0 - c.0).powi(2) + (q.1 - c.1).powi(2)).sqrt())
                .sum::<f64>()
                / p.len() as f64
        };
        assert!(
            between > 1.5 * spread(a, ca).max(spread(b, cb)),
            "between {between}, spread {}",
            spread(a, ca)
        );
    }

    #[test]
    fn same_input_same_layout() {
        let start: Vec<_> = (0..40).map(seed_position).collect();
        let e = two_clusters(20);
        let one = layout(start.clone(), &e, &[false; 40]).0;
        let two = layout(start, &e, &[false; 40]).0;
        assert_eq!(one, two);
    }

    #[test]
    fn pinned_nodes_stay() {
        let start: Vec<_> = (0..10).map(seed_position).collect();
        let mut pinned = [false; 10];
        pinned[3] = true;
        let (pos, _) = layout(start.clone(), &two_clusters(5), &pinned);
        assert_eq!(pos[3], start[3]);
        assert_ne!(pos[4], start[4]);
    }

    #[test]
    fn coincident_points_do_not_hang() {
        let start = vec![(0.0, 0.0); 50];
        let (pos, _) = layout(start, &[(0, 1)], &[false; 50]);
        assert!(pos.iter().all(|p| p.0.is_finite() && p.1.is_finite()));
    }
}
