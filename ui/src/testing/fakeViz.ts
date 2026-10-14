// A stand-in for the Rust scene builder (spec 022), for UI tests: the fake
// crawl's URLs as a tree where URL i hangs under URL floor((i - 1) / 4).
// Like the real one it opens nodes breadth first within a budget, honours
// the expanded keys, and lays leaves out in slots.
import type { Scene } from "../generated/Scene";
import type { TermLinks } from "../generated/TermLinks";
import type { SceneNode } from "../generated/SceneNode";
import type { ViewState } from "../generated/ViewState";
import type { FakeUrl } from "./fakeResults";

/** Small, so a fake crawl of a few dozen URLs has folded branches. */
export const FAKE_VIZ_BUDGET = 20;

export function fakeScene(urls: FakeUrl[], state: ViewState): Scene {
  if (state.view === "link_graph") return fakeGraph(urls, state);
  if (state.view === "anchors") return fakeAnchors(urls, state);
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
      width: null,
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
    edges: [],
    directed: false,
    settled: true,
    note: null,
    language: null,
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

/**
 * The link graph over the fake crawl: URL i links to its tree parent and to
 * URL i + 1. Pages sit on a circle; a focus keeps the pages within
 * `distance` links of it, with the focus in the middle; pinned keys stay
 * where they were dragged.
 */
function fakeGraph(urls: FakeUrl[], state: ViewState): Scene {
  const n = Math.min(urls.length, 400);
  const pairs: [number, number][] = [];
  for (let i = 1; i < n; i++) {
    pairs.push([i, Math.floor((i - 1) / 4)]);
    if (i + 1 < n) pairs.push([i, i + 1]);
  }
  let keep = Array.from({ length: n }, (_, i) => i);
  const focus = state.focus ? urls.findIndex((u) => `u:${u.id}` === state.focus) : -1;
  if (focus >= 0) {
    const near = new Set([focus]);
    for (let d = 0; d < Math.max(1, Math.min(2, state.distance)); d++)
      for (const [a, b] of pairs.filter(([a, b]) => near.has(a) || near.has(b))) near.add(a), near.add(b);
    keep = [...near].sort((a, b) => (a === focus ? -1 : b === focus ? 1 : a - b));
  } else keep = keep.slice(0, state.budget ?? FAKE_VIZ_BUDGET);
  const at = new Map(keep.map((i, k) => [i, k]));
  const nodes: SceneNode[] = keep.map((i, k) => {
    const u = urls[i];
    const key = `u:${u.id}`;
    const a = (k / keep.length) * Math.PI * 2;
    const [x, y] = state.pinned[key] ?? (k === 0 && focus >= 0 ? [0, 0] : [200 * Math.cos(a), 200 * Math.sin(a)]);
    return {
      key,
      label: i === 0 ? u.url : new URL(u.url).pathname,
      url: u.url,
      url_id: u.id,
      x,
      y,
      r: 5,
      width: null,
      category: u.status === 404 ? "client_error" : "ok",
      depth: i === 0 ? 0 : Math.floor(Math.log(i * 3 + 1) / Math.log(4)),
      score: null,
      inlinks: pairs.filter(([, b]) => b === i).length,
      count: 1,
      hidden: 0,
      expandable: false,
      expanded: false,
      parent: null,
    };
  });
  const edges = pairs.filter(([a, b]) => at.has(a) && at.has(b)).map(([a, b]) => ({ a: at.get(a)!, b: at.get(b)! }));
  const xs = nodes.map((p) => p.x);
  const ys = nodes.map((p) => p.y);
  const ok = keep.filter((i) => urls[i].status !== 404).length;
  return {
    view: "link_graph",
    layout: "force",
    nodes,
    edges,
    directed: focus >= 0,
    settled: true,
    note: null,
    language: null,
    legend: [
      { category: "ok", label: "Success (2xx)", count: ok },
      { category: "client_error", label: "Client error (4xx)", count: keep.length - ok },
    ],
    size_legend: state.size === "uniform" ? null : "Size: same for all (Link Score not worked out yet)",
    omitted: keep.length < n && focus < 0 ? { count: n - keep.length, reason: "Only the pages with the most links in are shown." } : null,
    bounds: { x: Math.min(...xs) - 5, y: Math.min(...ys) - 5, width: Math.max(...xs) - Math.min(...xs) + 10, height: Math.max(...ys) - Math.min(...ys) + 10 },
    total: n,
    snapshot_at: 1_760_000_000_000,
  };
}

/** Fake anchor terms and their counts: the start page has no inlinks. */
function fakeTerms(urls: FakeUrl[], state: ViewState): [string, number][] {
  if (state.focus === `u:${urls[0]?.id}`) return [];
  const words: [string, number][] = [
    ["running", 9],
    ["shoes", 8],
    ["sale", 5],
    ["guide", 4],
    ["blog", 3],
    ["women", 2],
    ["men", 2],
    ["click", 1],
  ];
  const terms: [string, number][] =
    state.terms === "whole"
      ? [
          ["running shoes", 6],
          ["click here", 3],
          ["sale", 2],
        ]
      : state.stop_words
        ? words
        : [...words.slice(0, 2), ["the", 7], ...words.slice(2), ["here", 1]];
  return terms.map(([t, c]) => [t, state.followed_only ? Math.max(1, c - 1) : c] as [string, number]).sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
}

function fakeAnchors(urls: FakeUrl[], state: ViewState): Scene {
  const terms = fakeTerms(urls, state);
  // In rows of three, biggest first.
  const nodes: SceneNode[] = terms.map(([t, c], i) => {
    const font = 12 + c * 3;
    return {
      key: `t:${t}`,
      label: t,
      url: null,
      url_id: null,
      x: (i % 3) * 160 - 160,
      y: Math.floor(i / 3) * 50,
      r: font / 2,
      width: t.length * font * 0.55 + 6,
      category: "other",
      depth: i,
      score: null,
      inlinks: null,
      count: c,
      hidden: 0,
      expandable: false,
      expanded: false,
      parent: null,
    };
  });
  const links = terms.reduce((a, [, c]) => a + c, 0);
  return {
    view: "anchors",
    layout: "cloud",
    nodes,
    edges: [],
    directed: false,
    settled: true,
    legend: [],
    size_legend: `Size: links that use the term (${links} with anchor text)`,
    omitted: null,
    bounds: nodes.length ? { x: -260, y: -20, width: 520, height: Math.ceil(nodes.length / 3) * 50 + 20 } : { x: 0, y: 0, width: 0, height: 0 },
    total: links,
    snapshot_at: 1_760_000_000_000,
    note: nodes.length ? null : "No internal links point to this page.",
    language: "en",
  };
}

/** The links behind a fake term: as many as its count, from the first URLs. */
export function fakeTermLinks(urls: FakeUrl[], state: ViewState, term: string, limit: number): TermLinks {
  const count = fakeTerms(urls, state).find(([t]) => t === term)?.[1] ?? 0;
  const to = urls.find((u) => `u:${u.id}` === state.focus) ?? urls[0];
  return {
    total: count,
    links: urls.slice(0, Math.min(count, limit)).map((u, i) => ({
      from_id: u.id,
      from_url: u.url,
      to_id: to.id,
      to_url: to.url,
      text: i === 1 ? `Best ${term}` : `${term[0].toUpperCase()}${term.slice(1)} today`,
      image: false,
      rel: i === 2 ? "nofollow" : null,
      nofollow: i === 2,
    })),
  };
}
