// The drawing surface of every visualisation (spec 022 plan D7): Canvas 2D,
// device-pixel-ratio aware, redrawn on change. Nodes are batched by shape
// and colour; labels only where they fit.
import { forwardRef, useCallback, useEffect, useImperativeHandle, useMemo, useRef, useState } from "react";
import type { Category } from "../generated/Category";
import type { Scene } from "../generated/Scene";
import type { SceneNode } from "../generated/SceneNode";
import { fit, HitGrid, labelled, nearest, pinTo, toScreen, toWorld, zoomAt, type Camera, type Direction } from "./geometry";

/** Colour token per category; shape per category for colour-blind safety (FR-005). */
export const categoryToken: Record<Category, string> = {
  ok: "--viz-ok",
  redirect: "--viz-redirect",
  client_error: "--viz-client-error",
  server_error: "--viz-server-error",
  blocked: "--viz-blocked",
  not_crawled: "--viz-not-crawled",
  other: "--viz-not-crawled",
  folder: "--viz-folder",
  group: "--viz-group",
};

type Shape = "circle" | "diamond" | "triangle" | "square" | "ring" | "dashed" | "rounded";
export const categoryShape: Record<Category, Shape> = {
  ok: "circle",
  redirect: "diamond",
  client_error: "triangle",
  server_error: "square",
  blocked: "ring",
  not_crawled: "dashed",
  other: "dashed",
  folder: "rounded",
  group: "ring",
};

/** Colour of a node when colouring by depth: the accent fading with depth. */
const depthColour = (depth: number) => `color-mix(in oklch, var(--accent) ${Math.max(20, 100 - depth * 16)}%, var(--fg-3))`;

function shapePath(p: Path2D, shape: Shape, x: number, y: number, r: number) {
  switch (shape) {
    case "diamond":
      p.moveTo(x, y - r * 1.25);
      p.lineTo(x + r * 1.25, y);
      p.lineTo(x, y + r * 1.25);
      p.lineTo(x - r * 1.25, y);
      p.closePath();
      break;
    case "triangle":
      p.moveTo(x, y - r * 1.2);
      p.lineTo(x + r * 1.1, y + r * 0.8);
      p.lineTo(x - r * 1.1, y + r * 0.8);
      p.closePath();
      break;
    case "square":
      p.rect(x - r * 0.9, y - r * 0.9, r * 1.8, r * 1.8);
      break;
    case "rounded":
      p.roundRect(x - r, y - r, r * 2, r * 2, r * 0.4);
      break;
    default:
      p.moveTo(x + r, y);
      p.arc(x, y, r, 0, Math.PI * 2);
  }
}

export interface VizCanvasHandle {
  fit(): void;
  zoom(factor: number): void;
  /** Centres the camera on a node. */
  reveal(index: number): void;
}

interface Props {
  scene: Scene;
  colour: "status" | "depth";
  selected: string | null;
  onSelect: (key: string | null) => void;
  /** Enter, double-click: open or close a branch, or show more; with where the node is on screen. */
  onActivate: (key: string, at: { sx: number; sy: number }) => void;
  /** Key of the node that was activated last: kept still on screen when the next scene arrives. */
  anchor: { key: string; sx: number; sy: number } | null;
  onHover?: (node: SceneNode | null, at: { x: number; y: number } | null) => void;
  /** The token values, read by the parent once per theme (tests pass fixed ones). */
  colours: Record<string, string>;
  /** Link graph: a node was dragged to world (x, y); with where it is on screen. */
  onPin?: (key: string, x: number, y: number, at: { sx: number; sy: number }) => void;
}

const viewName: Record<Scene["view"], string> = { crawl_tree: "Crawl tree", directory_tree: "Directory tree", link_graph: "Link graph" };

export const VizCanvas = forwardRef<VizCanvasHandle, Props>(function VizCanvas({ scene, colour, selected, onSelect, onActivate, anchor, onHover, colours, onPin }, ref) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
  const [cam, setCam] = useState<Camera>({ x: 0, y: 0, k: 1 });
  const [hover, setHover] = useState<number | null>(null);
  const grid = useMemo(() => new HitGrid(scene.nodes), [scene]);
  const index = useMemo(() => new Map(scene.nodes.map((n, i) => [n.key, i])), [scene]);
  const sel = selected === null ? null : (index.get(selected) ?? null);
  const drag = useRef<{ sx: number; sy: number; cam: Camera; moved: boolean; node: number | null } | null>(null);
  // A node being dragged (link graph): drawn here until the next scene pins it.
  const [held, setHeld] = useState<{ i: number; x: number; y: number } | null>(null);
  useEffect(() => setHeld(null), [scene]);

  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const measure = () => setSize({ w: el.clientWidth, h: el.clientHeight });
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // A new scene: keep the activated node still, else fit the whole scene.
  const sized = size.w > 0 && size.h > 0;
  useEffect(() => {
    if (!sized) return;
    const a = anchor && index.get(anchor.key);
    if (anchor && a !== undefined && a !== null) {
      const n = scene.nodes[a];
      setCam((c) => pinTo(c, n.x, n.y, anchor.sx, anchor.sy));
    } else setCam(fit(scene.bounds, size.w, size.h));
    // Only on a new scene or the first size.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scene, sized]);

  useImperativeHandle(
    ref,
    () => ({
      fit: () => setCam(fit(scene.bounds, size.w, size.h)),
      zoom: (f) => setCam((c) => zoomAt(c, size.w / 2, size.h / 2, f)),
      reveal: (i) => {
        const n = scene.nodes[i];
        if (n) setCam((c) => pinTo(c, n.x, n.y, size.w / 2, size.h / 2));
      },
    }),
    [scene, size],
  );

  // Draw.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !sized) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(size.w * dpr);
    canvas.height = Math.round(size.h * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, size.w, size.h);
    ctx.save();
    ctx.translate(cam.x, cam.y);
    ctx.scale(cam.k, cam.k);
    const nodes = held ? scene.nodes.map((n, i) => (i === held.i ? { ...n, x: held.x, y: held.y } : n)) : scene.nodes;
    const radial = scene.layout === "radial";
    // Lines, one path.
    const lines = new Path2D();
    for (const n of nodes) {
      if (n.parent === null) continue;
      const p = nodes[n.parent];
      lines.moveTo(p.x, p.y);
      if (radial) lines.lineTo(n.x, n.y);
      else {
        const mx = (p.x + n.x) / 2;
        lines.bezierCurveTo(mx, p.y, mx, n.y, n.x, n.y);
      }
    }
    // Link graph: straight lines, with arrowheads when the direction matters.
    const heads = new Path2D();
    for (const e of scene.edges) {
      const a = nodes[e.a];
      const b = nodes[e.b];
      lines.moveTo(a.x, a.y);
      lines.lineTo(b.x, b.y);
      if (!scene.directed) continue;
      const dx = b.x - a.x;
      const dy = b.y - a.y;
      const len = Math.hypot(dx, dy);
      if (len < b.r + 2) continue;
      const ux = dx / len;
      const uy = dy / len;
      const tx = b.x - ux * (b.r + 1.5);
      const ty = b.y - uy * (b.r + 1.5);
      const s = 5 / Math.max(cam.k, 0.5);
      heads.moveTo(tx, ty);
      heads.lineTo(tx - ux * s - uy * s * 0.5, ty - uy * s + ux * s * 0.5);
      heads.lineTo(tx - ux * s + uy * s * 0.5, ty - uy * s - ux * s * 0.5);
      heads.closePath();
    }
    ctx.strokeStyle = colours["--viz-line"];
    ctx.lineWidth = 1 / cam.k;
    ctx.stroke(lines);
    ctx.fillStyle = colours["--viz-line"];
    ctx.fill(heads);
    // Nodes, one path per shape and colour.
    const batches = new Map<string, { shape: Shape; fill: string; path: Path2D }>();
    for (const n of nodes) {
      const shape = categoryShape[n.category];
      const fill = colour === "depth" && n.url_id !== null ? depthColour(n.depth) : colours[categoryToken[n.category]];
      const key = `${shape}|${fill}`;
      let b = batches.get(key);
      if (!b) batches.set(key, (b = { shape, fill, path: new Path2D() }));
      shapePath(b.path, shape, n.x, n.y, n.r);
    }
    for (const b of batches.values()) {
      if (b.shape === "ring" || b.shape === "dashed") {
        ctx.setLineDash(b.shape === "dashed" ? [3 / cam.k, 2 / cam.k] : []);
        ctx.strokeStyle = b.fill;
        ctx.lineWidth = 2 / cam.k;
        ctx.stroke(b.path);
        ctx.setLineDash([]);
      } else {
        ctx.fillStyle = b.fill;
        ctx.fill(b.path);
      }
    }
    // Folded branches: a small plus-ring around the node.
    const folded = new Path2D();
    for (const n of nodes) if (n.expandable && !n.expanded && n.key.startsWith("m:") === false) folded.moveTo(n.x + n.r + 3, n.y), folded.arc(n.x, n.y, n.r + 3, 0, Math.PI * 2);
    ctx.strokeStyle = colours["--viz-line"];
    ctx.lineWidth = 1 / cam.k;
    ctx.stroke(folded);
    // Selection and hover rings.
    for (const [i, w] of [
      [hover, 1.5],
      [sel, 2.5],
    ] as const) {
      if (i === null) continue;
      const n = nodes[i];
      ctx.beginPath();
      ctx.arc(n.x, n.y, n.r + 5, 0, Math.PI * 2);
      ctx.strokeStyle = colours["--accent"];
      ctx.lineWidth = w / cam.k;
      ctx.stroke();
    }
    ctx.restore();
    // Labels in screen space, so text stays crisp.
    const w0 = toWorld(cam, 0, 0);
    const w1 = toWorld(cam, size.w, size.h);
    ctx.font = `12px ${colours["--font-ui"] || "system-ui"}`;
    ctx.fillStyle = colours["--fg-2"];
    // A halo in the background colour keeps labels readable over lines.
    ctx.strokeStyle = colours["--bg"];
    ctx.lineWidth = 3;
    ctx.lineJoin = "round";
    ctx.textBaseline = "middle";
    for (const i of labelled(nodes, cam.k, 300, 4, { x0: w0.x, y0: w0.y, x1: w1.x, y1: w1.y })) {
      const n = nodes[i];
      const s = toScreen(cam, n.x, n.y);
      const r = n.r * cam.k;
      const text = n.hidden > 0 && !n.key.startsWith("m:") ? `${n.label}  (+${n.hidden.toLocaleString()})` : n.label;
      ctx.strokeText(text, s.x + r + 6, s.y, 260);
      ctx.fillText(text, s.x + r + 6, s.y, 260);
    }
  }, [scene, cam, size, sized, hover, sel, colour, colours, held]);

  const local = (e: { clientX: number; clientY: number }) => {
    const r = canvasRef.current!.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };
  const hit = useCallback(
    (sx: number, sy: number) => {
      const w = toWorld(cam, sx, sy);
      return grid.at(w.x, w.y, 4 / cam.k);
    },
    [cam, grid],
  );

  const activate = (i: number) => {
    const n = scene.nodes[i];
    const s = toScreen(cam, n.x, n.y);
    onActivate(n.key, { sx: s.x, sy: s.y });
  };

  const move = (dir: Direction) => {
    if (scene.nodes.length === 0) return;
    const next = sel === null ? 0 : nearest(scene.nodes, sel, dir);
    if (next === null) return;
    onSelect(scene.nodes[next].key);
    const n = scene.nodes[next];
    const s = toScreen(cam, n.x, n.y);
    if (s.x < 24 || s.y < 24 || s.x > size.w - 24 || s.y > size.h - 24) setCam((c) => pinTo(c, n.x, n.y, size.w / 2, size.h / 2));
  };

  return (
    <div ref={wrapRef} className="relative h-full w-full overflow-hidden">
      <canvas
        ref={canvasRef}
        role="application"
        aria-label={`${viewName[scene.view]}: ${scene.nodes.length.toLocaleString()} nodes${scene.view === "link_graph" ? `, ${scene.edges.length.toLocaleString()} links` : ""}. Arrow keys move, Enter ${scene.view === "link_graph" ? "centres on a page" : "opens or closes a branch"}, plus and minus zoom, 0 fits.`}
        aria-activedescendant={undefined}
        tabIndex={0}
        className="block h-full w-full cursor-grab touch-none outline-none focus-visible:ring-2 focus-visible:ring-accent active:cursor-grabbing"
        style={{ width: size.w, height: size.h }}
        data-camera={`${cam.x.toFixed(1)},${cam.y.toFixed(1)},${cam.k.toFixed(3)}`}
        data-layout={scene.layout}
        onPointerDown={(e) => {
          (e.target as HTMLElement).setPointerCapture?.(e.pointerId);
          const p = local(e);
          drag.current = { sx: p.x, sy: p.y, cam, moved: false, node: onPin ? hit(p.x, p.y) : null };
        }}
        onPointerMove={(e) => {
          const p = local(e);
          const d = drag.current;
          if (d) {
            const dx = p.x - d.sx;
            const dy = p.y - d.sy;
            if (Math.abs(dx) + Math.abs(dy) > 3) d.moved = true;
            if (!d.moved) return;
            if (d.node !== null) {
              const w = toWorld(cam, p.x, p.y);
              setHeld({ i: d.node, x: w.x, y: w.y });
            } else setCam({ ...d.cam, x: d.cam.x + dx, y: d.cam.y + dy });
            return;
          }
          const i = hit(p.x, p.y);
          setHover(i);
          onHover?.(i === null ? null : scene.nodes[i], i === null ? null : p);
        }}
        onPointerUp={(e) => {
          const d = drag.current;
          drag.current = null;
          const p = local(e);
          if (d && !d.moved) {
            const i = hit(p.x, p.y);
            onSelect(i === null ? null : scene.nodes[i].key);
          } else if (d && d.node !== null && onPin) {
            const w = toWorld(cam, p.x, p.y);
            onPin(scene.nodes[d.node].key, w.x, w.y, { sx: p.x, sy: p.y });
          }
        }}
        onPointerLeave={() => {
          setHover(null);
          onHover?.(null, null);
        }}
        onDoubleClick={(e) => {
          const p = local(e);
          const i = hit(p.x, p.y);
          if (i !== null && (scene.nodes[i].expandable || scene.view === "link_graph")) activate(i);
        }}
        onWheel={(e) => {
          const p = local(e);
          setCam((c) => zoomAt(c, p.x, p.y, Math.exp(-e.deltaY * 0.0015)));
        }}
        onKeyDown={(e) => {
          const arrows: Record<string, Direction> = { ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down" };
          if (arrows[e.key]) {
            e.preventDefault();
            move(arrows[e.key]);
          } else if (e.key === "Enter" && sel !== null) {
            e.preventDefault();
            if (scene.nodes[sel].expandable || scene.view === "link_graph") activate(sel);
          } else if (e.key === "+" || e.key === "=") setCam((c) => zoomAt(c, size.w / 2, size.h / 2, 1.25));
          else if (e.key === "-") setCam((c) => zoomAt(c, size.w / 2, size.h / 2, 0.8));
          else if (e.key === "0") setCam(fit(scene.bounds, size.w, size.h));
          else if (e.key === "Escape") onSelect(null);
        }}
      />
    </div>
  );
});
