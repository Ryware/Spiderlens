// Camera, hit testing and keyboard navigation for the visualisation canvas
// (spec 022 plan D7). Pure functions, so the maths is tested without a canvas.
import type { Bounds } from "../generated/Bounds";
import type { SceneNode } from "../generated/SceneNode";

/** Screen = world * k + (x, y). */
export interface Camera {
  x: number;
  y: number;
  k: number;
}

export const MIN_ZOOM = 0.02;
export const MAX_ZOOM = 8;

const clampZoom = (k: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, k));

/** The camera that shows all of `b` in a `w` x `h` viewport with a margin. */
export function fit(b: Bounds, w: number, h: number, margin = 32): Camera {
  const bw = Math.max(b.width, 1);
  const bh = Math.max(b.height, 1);
  const k = clampZoom(Math.min((w - 2 * margin) / bw, (h - 2 * margin) / bh, 1.5));
  return { k, x: (w - bw * k) / 2 - b.x * k, y: (h - bh * k) / 2 - b.y * k };
}

export const toWorld = (c: Camera, sx: number, sy: number) => ({ x: (sx - c.x) / c.k, y: (sy - c.y) / c.k });
export const toScreen = (c: Camera, wx: number, wy: number) => ({ x: wx * c.k + c.x, y: wy * c.k + c.y });

/** Zooms by `factor` keeping the world point under (sx, sy) still. */
export function zoomAt(c: Camera, sx: number, sy: number, factor: number): Camera {
  const k = clampZoom(c.k * factor);
  const w = toWorld(c, sx, sy);
  return { k, x: sx - w.x * k, y: sy - w.y * k };
}

/** Moves the camera so world point (wx, wy) is at screen (sx, sy). */
export const pinTo = (c: Camera, wx: number, wy: number, sx: number, sy: number): Camera => ({ k: c.k, x: sx - wx * c.k, y: sy - wy * c.k });

/** A uniform grid over the scene for O(1) hover and click (plan D7). */
export class HitGrid {
  private cells = new Map<string, number[]>();
  private size: number;

  constructor(private nodes: SceneNode[], cell = 24) {
    this.size = cell;
    nodes.forEach((n, i) => {
      const r = n.r;
      for (let gx = Math.floor((n.x - r) / cell); gx <= Math.floor((n.x + r) / cell); gx++)
        for (let gy = Math.floor((n.y - r) / cell); gy <= Math.floor((n.y + r) / cell); gy++) {
          const k = `${gx},${gy}`;
          const list = this.cells.get(k);
          if (list) list.push(i);
          else this.cells.set(k, [i]);
        }
    });
  }

  /** The node under world point (x, y), within `slop` world units; topmost (last drawn) wins. */
  at(x: number, y: number, slop = 0): number | null {
    let best: number | null = null;
    let bestD = Infinity;
    const s = this.size;
    for (let gx = Math.floor((x - slop) / s); gx <= Math.floor((x + slop) / s); gx++)
      for (let gy = Math.floor((y - slop) / s); gy <= Math.floor((y + slop) / s); gy++)
        for (const i of this.cells.get(`${gx},${gy}`) ?? []) {
          const n = this.nodes[i];
          const d = Math.hypot(n.x - x, n.y - y);
          if (d <= n.r + slop && (d < bestD || (d === bestD && i > (best ?? -1)))) {
            best = i;
            bestD = d;
          }
        }
    return best;
  }
}

export type Direction = "left" | "right" | "up" | "down";

/**
 * The node to move to from `from` with an arrow key: the nearest node in that
 * direction, preferring nodes close to the straight line.
 */
export function nearest(nodes: SceneNode[], from: number, dir: Direction): number | null {
  const o = nodes[from];
  let best: number | null = null;
  let bestCost = Infinity;
  nodes.forEach((n, i) => {
    if (i === from) return;
    const dx = n.x - o.x;
    const dy = n.y - o.y;
    const along = dir === "right" ? dx : dir === "left" ? -dx : dir === "down" ? dy : -dy;
    const across = dir === "left" || dir === "right" ? Math.abs(dy) : Math.abs(dx);
    if (along <= 0) return;
    const cost = along + 2 * across;
    if (cost < bestCost) {
      bestCost = cost;
      best = i;
    }
  });
  return best;
}

/**
 * Which nodes get a label (FR-022): on-screen radius of at least `minR` px,
 * at most `budget`, larger and higher in the tree first.
 */
export function labelled(nodes: SceneNode[], k: number, budget = 300, minR = 4, view?: { x0: number; y0: number; x1: number; y1: number }): number[] {
  const out: number[] = [];
  nodes.forEach((n, i) => {
    if (n.r * k < minR) return;
    if (view && (n.x < view.x0 || n.x > view.x1 || n.y < view.y0 || n.y > view.y1)) return;
    out.push(i);
  });
  out.sort((a, b) => nodes[b].r - nodes[a].r || nodes[a].depth - nodes[b].depth || a - b);
  return out.slice(0, budget);
}
