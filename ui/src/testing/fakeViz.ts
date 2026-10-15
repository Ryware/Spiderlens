// A stand-in for the Rust scene builder (spec 022), for UI tests: the fake
// crawl's URLs as a tree where URL i hangs under URL floor((i - 1) / 4).
// Like the real one it opens nodes breadth first within a budget, honours
// the expanded keys, and lays leaves out in slots.
import type { Scene } from "../generated/Scene";
import type { SceneNode } from "../generated/SceneNode";
import type { ViewState } from "../generated/ViewState";
import type { FakeUrl } from "./fakeResults";

/** Small, so a fake crawl of a few dozen URLs has folded branches. */
export const FAKE_VIZ_BUDGET = 20;

export function fakeScene(urls: FakeUrl[], state: ViewState): Scene {
  const tree = state.view === "directory_tree";
  const n = Math.min(urls.length, 400);
  const kids: number[][] = Array.from({ length: n }, () => []);
  for (let i = 1; i < n; i++) kids[Math.floor((i - 1) / 4)].push(i);
  const desc = new Array(n).fill(0);
  for (let i = n - 1; i > 0; i--) desc[Math.floor((i - 1) / 4)] += 1 + desc[i];
  const key = (i: number) => (tree && kids[i].length > 0 ? `d:${i + 1}` : `u:${urls[i].id}`);
  const wanted = state.expanded ? new Set(state.expanded) : null;
  // Asked-for nodes open whatever the budget, as in Rust.
  const budget = wanted ? 1000 : (state.budget ?? FAKE_VIZ_BUDGET);
  const open = new Set<number>();
  let count = 1;
  const q = [0];
  while (q.length) {
    const i = q.shift()!;
    if (kids[i].length === 0) continue;
    if (i !== 0 && wanted && !wanted.has(key(i))) continue;
    if (count + kids[i].length > budget) continue;
    count += kids[i].length;
    open.add(i);
    q.push(...kids[i]);
  }
  // Preorder with leaf slots.
  const order: { i: number; parent: number | null; level: number }[] = [];
  const walk = (i: number, parent: number | null, level: number) => {
    const me = order.length;
    order.push({ i, parent, level });
    if (open.has(i)) for (const c of kids[i]) walk(c, me, level + 1);
  };
  walk(0, null, 0);
  const slot: number[] = new Array(order.length).fill(0);
  let next = 0;
  const hasKid = new Set(order.filter((o) => o.parent !== null).map((o) => o.parent!));
  order.forEach((_, k) => {
    if (!hasKid.has(k)) slot[k] = next++;
  });
  for (let k = order.length - 1; k >= 0; k--) {
    if (!hasKid.has(k)) continue;
    const ch = order.map((o, j) => (o.parent === k ? slot[j] : null)).filter((x): x is number => x !== null);
    slot[k] = (Math.min(...ch) + Math.max(...ch)) / 2;
  }
  const radial = state.layout === "radial";
  const leaves = Math.max(next, 1);
  const nodes: SceneNode[] = order.map(({ i, parent, level }, k) => {
    const u = urls[i];
    const folder = tree && kids[i].length > 0;
    const a = (slot[k] / leaves) * Math.PI * 2;
    const [x, y] = radial ? (level === 0 ? [0, 0] : [level * 160 * Math.cos(a), level * 160 * Math.sin(a)]) : [level * 220, slot[k] * 22];
    return {
      key: key(i),
      label: i === 0 ? u.url : folder ? `section-${i}/` : new URL(u.url).pathname,
      url: folder ? null : u.url,
      url_id: folder ? null : u.id,
      x,
      y,
      r: 5,
      category: folder ? "folder" : u.status === 404 ? "client_error" : "ok",
      depth: level,
      score: null,
      inlinks: null,
      count: 1 + desc[i],
      hidden: open.has(i) ? 0 : desc[i],
      expandable: kids[i].length > 0,
      expanded: open.has(i),
      parent,
    };
  });
  const xs = nodes.map((p) => p.x);
  const ys = nodes.map((p) => p.y);
  const shown = nodes.length;
  const ok = urls.slice(0, n).filter((u) => u.status !== 404).length;
  return {
    view: state.view,
    layout: state.layout,
    nodes,
    legend: [
      { category: "ok", label: "Success (2xx)", count: ok },
      { category: "client_error", label: "Client error (4xx)", count: n - ok },
    ],
    size_legend: state.size === "uniform" ? null : "Size: same for all (Link Score not worked out yet)",
    omitted: shown < n ? { count: n - shown, reason: "The rest are in folded branches: open one to see its URLs." } : null,
    bounds: { x: Math.min(...xs) - 5, y: Math.min(...ys) - 5, width: Math.max(...xs) - Math.min(...xs) + 10, height: Math.max(...ys) - Math.min(...ys) + 10 },
    total: n,
    snapshot_at: 1_760_000_000_000,
  };
}
