// A visualisation in the main area (spec 022 plan D11): toolbar, canvas,
// legend, what was left out, and a tooltip. Rust builds every scene; this
// only asks for the next one and draws it.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Crosshair, Globe, Maximize2, Minus, Plus, RefreshCw, RotateCcw, X } from "lucide-react";
import type { ColourBy } from "../generated/ColourBy";
import type { Layout } from "../generated/Layout";
import type { Scene } from "../generated/Scene";
import type { SceneNode } from "../generated/SceneNode";
import type { SizeBy } from "../generated/SizeBy";
import type { TermLinks } from "../generated/TermLinks";
import type { Terms } from "../generated/Terms";
import type { ViewKind } from "../generated/ViewKind";
import type { ViewState } from "../generated/ViewState";
import type { Transport } from "../transport";
import { Button, cn, Input, Notice, Segmented, Select, Switch } from "../components/ui";
import { categoryShape, categoryToken, VizCanvas, type VizCanvasHandle } from "./VizCanvas";

export const vizViews: { id: string; view: ViewKind; title: string }[] = [
  { id: "crawl-tree", view: "crawl_tree", title: "Crawl tree" },
  { id: "directory-tree", view: "directory_tree", title: "Directory tree" },
  { id: "link-graph", view: "link_graph", title: "Link graph" },
  { id: "anchor-text", view: "anchors", title: "Anchor text" },
];

const TOKENS = [...new Set(Object.values(categoryToken)), "--viz-line", "--accent", "--accent-text", "--fg-2", "--fg-3", "--bg", "--font-ui"];

/** The token values the canvas draws with, read again when the theme changes. */
function useTokenColours(): Record<string, string> {
  const read = () => {
    const css = getComputedStyle(document.documentElement);
    return Object.fromEntries(TOKENS.map((t) => [t, css.getPropertyValue(t).trim() || "currentColor"]));
  };
  const [colours, setColours] = useState(read);
  useEffect(() => {
    const mo = new MutationObserver(() => setColours(read()));
    mo.observe(document.documentElement, { attributes: true });
    const mq = window.matchMedia?.("(prefers-color-scheme: dark)");
    const on = () => setColours(read());
    mq?.addEventListener?.("change", on);
    return () => {
      mo.disconnect();
      mq?.removeEventListener?.("change", on);
    };
  }, []);
  return colours;
}

/** A small swatch in the legend, in the node's shape. */
function Swatch({ category, colour }: { category: SceneNode["category"]; colour: string }) {
  const shape = categoryShape[category];
  const common = { fill: shape === "ring" || shape === "dashed" ? "none" : colour, stroke: colour, strokeWidth: shape === "ring" || shape === "dashed" ? 1.5 : 0 };
  return (
    <svg aria-hidden width="12" height="12" viewBox="-6 -6 12 12" className="shrink-0">
      {shape === "diamond" ? (
        <path d="M0 -5 L5 0 L0 5 L-5 0 Z" {...common} />
      ) : shape === "triangle" ? (
        <path d="M0 -5 L4.5 3.5 L-4.5 3.5 Z" {...common} />
      ) : shape === "square" ? (
        <rect x="-4" y="-4" width="8" height="8" {...common} />
      ) : shape === "rounded" ? (
        <rect x="-4.5" y="-4.5" width="9" height="9" rx="2" {...common} />
      ) : (
        <circle r="4" strokeDasharray={shape === "dashed" ? "2 1.5" : undefined} {...common} />
      )}
    </svg>
  );
}

export function VizView({
  transport,
  path,
  view,
  live,
  focusId,
  onFocusId,
}: {
  transport: Transport;
  path: string;
  view: ViewKind;
  live: boolean;
  focusId: number | null;
  onFocusId: (id: number | null) => void;
}) {
  const graph = view === "link_graph";
  const cloud = view === "anchors";
  const [layout, setLayout] = useState<Layout>(graph ? "force" : cloud ? "cloud" : "tree");
  const [colour, setColour] = useState<ColourBy>("status");
  // The graph sizes pages by Link Score unless asked otherwise.
  const [size, setSize] = useState<SizeBy>(graph ? "score" : "uniform");
  // The anchor text cloud opens on the selected page, else the whole site.
  const [focus, setFocus] = useState<string | null>(cloud && focusId !== null ? `u:${focusId}` : null);
  const [terms, setTerms] = useState<Terms>("words");
  const [stopWords, setStopWords] = useState(true);
  const [followedOnly, setFollowedOnly] = useState(false);
  const [distance, setDistance] = useState(1);
  const [pinned, setPinned] = useState<Record<string, [number, number]>>({});
  const [expanded, setExpanded] = useState<string[] | null>(null);
  const [pages, setPages] = useState<Record<string, number>>({});
  const [scene, setScene] = useState<Scene | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const [selected, setSelected] = useState<string | null>(null);
  const [anchor, setAnchor] = useState<{ key: string; sx: number; sy: number } | null>(null);
  const [tip, setTip] = useState<{ node: SceneNode; x: number; y: number } | null>(null);
  const [search, setSearch] = useState("");
  const canvas = useRef<VizCanvasHandle>(null);
  const colours = useTokenColours();

  const state: ViewState = useMemo(
    () => ({ view, layout, colour, size, expanded, pages, budget: null, focus, distance, pinned, terms, stop_words: stopWords, followed_only: followedOnly }),
    [view, layout, colour, size, expanded, pages, focus, distance, pinned, terms, stopWords, followedOnly],
  );
  useEffect(() => {
    let off = false;
    setLoading(true);
    setError(null);
    transport.vizScene(path, state).then(
      (s) => {
        if (off) return;
        setScene(s);
        setLoading(false);
      },
      (e) => {
        if (off) return;
        setError(String(e));
        setLoading(false);
      },
    );
    return () => {
      off = true;
    };
  }, [transport, path, state, refresh]);

  // The inspector follows the selection (FR-002), and the reverse.
  const select = useCallback(
    (key: string | null) => {
      setSelected(key);
      const n = key && scene?.nodes.find((x) => x.key === key);
      if (n && n.url_id !== null) onFocusId(n.url_id);
    },
    [scene, onFocusId],
  );
  useEffect(() => {
    if (!scene || focusId === null || cloud) return;
    const n = scene.nodes.find((x) => x.url_id === focusId);
    if (n) setSelected(n.key);
  }, [scene, focusId]);

  // Link graph: centre on a page and its neighbours; a new picture, so the
  // dragged positions go.
  const centre = (key: string | null) => {
    setAnchor(null);
    setPinned({});
    setFocus(key);
  };

  const activate = (key: string, at: { sx: number; sy: number }) => {
    if (!scene) return;
    if (graph) {
      if (key.startsWith("u:")) centre(key);
      return;
    }
    if (cloud) return;
    setAnchor({ key, ...at });
    if (key.startsWith("m:")) {
      // "+N more": the next page of the parent's children.
      const parent = key.slice(2);
      setPages((p) => ({ ...p, [parent]: (p[parent] ?? 1) + 1 }));
      setAnchor(null);
      return;
    }
    const open = scene.nodes.filter((n) => n.expanded).map((n) => n.key);
    const node = scene.nodes.find((n) => n.key === key);
    setExpanded(node?.expanded ? open.filter((k) => k !== key) : [...open, key]);
  };

  const find = (q: string) => {
    if (!scene || !q.trim()) return;
    const needle = q.trim().toLowerCase();
    const i = scene.nodes.findIndex((n) => n.label.toLowerCase().includes(needle) || n.url?.toLowerCase().includes(needle));
    if (i < 0) return;
    select(scene.nodes[i].key);
    canvas.current?.reveal(i);
  };

  const title = vizViews.find((v) => v.view === view)?.title ?? "Visualisation";
  return (
    <section aria-label={title} className="flex h-full flex-col">
      <div role="toolbar" aria-label={`${title} options`} className="flex shrink-0 flex-wrap items-center gap-2 border-b border-line px-3 py-2">
        {cloud ? (
          <>
            <Segmented
              label="Anchor text of"
              size="sm"
              value={focus ? "page" : "site"}
              onChange={(v) => (setSelected(null), setFocus(v === "page" && focusId !== null ? `u:${focusId}` : null))}
              options={[
                { value: "page", label: "Links to this page", disabled: focusId === null && focus === null },
                { value: "site", label: "Whole site" },
              ]}
            />
            {focus && focusId !== null && focus !== `u:${focusId}` && (
              <Button variant="quiet" size="sm" onClick={() => (setSelected(null), setFocus(`u:${focusId}`))}>
                <Crosshair className="size-3.5" /> Use selected page
              </Button>
            )}
            <Segmented
              label="Terms"
              size="sm"
              value={terms}
              onChange={(v) => (setSelected(null), setTerms(v))}
              options={[
                { value: "words", label: "Words" },
                { value: "whole", label: "Whole anchors" },
              ]}
            />
            <label className="flex items-center gap-1.5 text-sm text-fg-2">
              <Switch checked={stopWords} onChange={(v) => (setSelected(null), setStopWords(v))} label="Leave out stop words" disabled={terms === "whole"} />
              Leave out stop words
            </label>
            <label className="flex items-center gap-1.5 text-sm text-fg-2">
              <Switch checked={followedOnly} onChange={(v) => (setSelected(null), setFollowedOnly(v))} label="Followed links only" />
              Followed only
            </label>
          </>
        ) : graph ? (
          <>
            {focus ? (
              <>
                <span className="max-w-56 truncate text-sm text-fg-2" title={scene?.nodes.find((n) => n.key === focus)?.url ?? undefined}>
                  Around {scene?.nodes.find((n) => n.key === focus)?.label ?? "the page"}
                </span>
                <Segmented
                  label="Distance"
                  size="sm"
                  value={String(distance)}
                  onChange={(v) => (setAnchor(null), setPinned({}), setDistance(Number(v)))}
                  options={[
                    { value: "1", label: "1 link" },
                    { value: "2", label: "2 links" },
                  ]}
                />
                <Button variant="quiet" size="sm" onClick={() => centre(null)}>
                  <Globe className="size-3.5" /> Whole site
                </Button>
              </>
            ) : (
              <Button variant="quiet" size="sm" disabled={!selected?.startsWith("u:")} onClick={() => selected && centre(selected)}>
                <Crosshair className="size-3.5" /> Focus on selected
              </Button>
            )}
            <Button variant="quiet" size="sm" disabled={Object.keys(pinned).length === 0} onClick={() => (setAnchor(null), setPinned({}))}>
              <RotateCcw className="size-3.5" /> Reset layout
            </Button>
          </>
        ) : (
        <Segmented
          label="Layout"
          size="sm"
          value={layout}
          onChange={(v) => {
            setAnchor(null);
            setLayout(v);
          }}
          options={[
            { value: "tree", label: "Tree" },
            { value: "radial", label: "Radial" },
          ]}
        />
        )}
        {!cloud && (
        <Select aria-label="Colour" className="w-36" value={colour} onChange={(e) => setColour(e.target.value as ColourBy)}>
          <option value="status">Colour: status</option>
          <option value="depth">Colour: depth</option>
        </Select>
        )}
        {!cloud && (
        <Select aria-label="Size" className="w-40" value={size} onChange={(e) => setSize(e.target.value as SizeBy)}>
          {!graph && <option value="uniform">Size: same</option>}
          <option value="score">Size: Link Score</option>
          <option value="inlinks">Size: inlinks</option>
          {graph && <option value="uniform">Size: same</option>}
        </Select>
        )}
        <form
          className="flex min-w-40 flex-1"
          onSubmit={(e) => {
            e.preventDefault();
            find(search);
          }}
        >
          <Input aria-label="Find in view" placeholder={cloud ? "Find a term" : "Find a URL or folder"} className="w-full max-w-72" value={search} onChange={(e) => setSearch(e.target.value)} />
        </form>
        <div className="ml-auto flex items-center gap-1">
          {loading && <span className="text-xs text-fg-3">Working…</span>}
          {scene && live && <span className="text-xs text-fg-3">As of {new Date(scene.snapshot_at).toLocaleTimeString()}</span>}
          <Button variant="quiet" icon size="sm" aria-label="Zoom out" onClick={() => canvas.current?.zoom(0.8)}>
            <Minus className="size-3.5" />
          </Button>
          <Button variant="quiet" icon size="sm" aria-label="Zoom in" onClick={() => canvas.current?.zoom(1.25)}>
            <Plus className="size-3.5" />
          </Button>
          <Button variant="quiet" icon size="sm" aria-label="Fit to view" onClick={() => canvas.current?.fit()}>
            <Maximize2 className="size-3.5" />
          </Button>
          <Button variant="quiet" icon size="sm" aria-label="Refresh" onClick={() => (setAnchor(null), setRefresh((r) => r + 1))}>
            <RefreshCw className="size-3.5" />
          </Button>
        </div>
      </div>
      <div className="flex min-h-0 flex-1">
      <div className="relative min-w-0 flex-1">
        {error && (
          <div className="p-3">
            <Notice kind="bad">{error}</Notice>
          </div>
        )}
        {scene && scene.nodes.length === 0 && <div className="p-6 text-sm text-fg-3">{scene.note ?? "No URLs to show yet."}</div>}
        {scene && scene.nodes.length > 0 && (
          <VizCanvas
            ref={canvas}
            scene={scene}
            colour={colour}
            selected={selected}
            onSelect={select}
            onActivate={activate}
            anchor={anchor}
            colours={colours}
            onHover={(node, at) => setTip(node && at ? { node, ...at } : null)}
            onPin={
              graph
                ? (key, x, y, at) => {
                    setAnchor({ key, ...at });
                    setPinned((p) => ({ ...p, [key]: [x, y] }));
                  }
                : undefined
            }
          />
        )}
        {tip && !cloud && (
          <div role="tooltip" className="pointer-events-none absolute z-10 max-w-80 rounded-md border border-line bg-surface px-2 py-1 text-xs shadow-raised" style={{ left: tip.x + 14, top: tip.y + 10 }}>
            <div className="truncate font-medium text-fg">{tip.node.url ?? tip.node.label}</div>
            <div className="text-fg-3">
              {scene?.legend.find((l) => l.category === tip.node.category)?.label ?? (tip.node.category === "folder" ? "Folder" : "")}
              {tip.node.count > 1 && ` · ${tip.node.count.toLocaleString()} URLs`}
              {tip.node.score !== null && ` · Link Score ${tip.node.score.toFixed(1)}`}
              {graph && tip.node.inlinks !== null && ` · ${tip.node.inlinks.toLocaleString()} links in`}
              {tip.node.hidden > 0 && ` · ${tip.node.hidden.toLocaleString()} folded`}
            </div>
          </div>
        )}
        {scene && scene.nodes.length > 0 && (
          <div aria-label="Legend" role="group" className="absolute bottom-3 left-3 flex max-w-72 flex-col gap-1 rounded-md border border-line bg-surface/95 px-2.5 py-2 text-xs shadow-raised">
            {cloud ? (
              <div className="text-fg-2">
                {focus ? "Words other pages use when they link here" : "Words used in internal links"}
                {scene.language && ` · stop words: ${scene.language}`}
              </div>
            ) : colour === "status" ? (
              scene.legend.map((l) => (
                <div key={l.category} className="flex items-center gap-2">
                  <Swatch category={l.category} colour={colours[categoryToken[l.category]]} />
                  <span className="min-w-0 flex-1 truncate text-fg-2">{l.label}</span>
                  <span className="tabular text-fg-3">{l.count.toLocaleString()}</span>
                </div>
              ))
            ) : (
              <div className="text-fg-2">Colour: crawl depth, darker is closer to the start</div>
            )}
            {scene.size_legend && <div className="text-fg-3">{scene.size_legend}</div>}
          </div>
        )}
        {scene?.omitted && (
          <div role="note" className={cn("absolute right-3 bottom-3 max-w-80 rounded-md border border-line bg-surface/95 px-2.5 py-2 text-xs text-fg-2 shadow-raised")}>
            {cloud
              ? `Showing ${scene.nodes.length.toLocaleString()} of ${(scene.nodes.length + scene.omitted.count).toLocaleString()} terms. `
              : `Showing ${(scene.total - scene.omitted.count).toLocaleString()} of ${scene.total.toLocaleString()} URLs. `}
            {scene.omitted.reason}
          </div>
        )}
      </div>
        {cloud && selected?.startsWith("t:") && (
          <TermList transport={transport} path={path} state={state} term={selected.slice(2)} onFocusId={onFocusId} onClose={() => setSelected(null)} />
        )}
      </div>
    </section>
  );
}

/** The links behind one term (FR-018): source page, full anchor text, rel. */
function TermList({
  transport,
  path,
  state,
  term,
  onFocusId,
  onClose,
}: {
  transport: Transport;
  path: string;
  state: ViewState;
  term: string;
  onFocusId: (id: number | null) => void;
  onClose: () => void;
}) {
  const [links, setLinks] = useState<TermLinks | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let off = false;
    setLinks(null);
    setError(null);
    transport.vizTermLinks(path, state, term).then(
      (l) => !off && setLinks(l),
      (e) => !off && setError(String(e)),
    );
    return () => {
      off = true;
    };
  }, [transport, path, state, term]);
  return (
    <div role="region" aria-label={`Links using ${term}`} className="flex w-80 max-w-2/5 shrink-0 flex-col border-l border-line bg-surface">
      <div className="flex shrink-0 items-center gap-2 border-b border-line px-3 py-2">
        <div className="min-w-0 flex-1 truncate text-sm font-medium text-fg" dir="auto">
          “{term}”
        </div>
        {links && <span className="text-xs text-fg-3 tabular">{links.total.toLocaleString()} links</span>}
        <Button variant="quiet" icon size="sm" aria-label="Close" onClick={onClose}>
          <X className="size-3.5" />
        </Button>
      </div>
      <div className="min-h-0 flex-1 overflow-auto px-3 py-1">
        {error && <Notice kind="bad">{error}</Notice>}
        {!links && !error && <div className="py-1 text-sm text-fg-3">Loading…</div>}
        {links && (
          <ul className="m-0 flex list-none flex-col p-0" data-selectable>
            {links.links.map((l, i) => (
              <li key={`${l.from_id}-${i}`} className="border-b border-line py-1.5 text-sm last:border-b-0">
                <button type="button" className="block w-full truncate text-left text-fg hover:text-accent-text" title={l.from_url} onClick={() => onFocusId(l.from_id)}>
                  {l.from_url}
                </button>
                <div className="flex items-baseline gap-2 text-xs text-fg-3">
                  <span className="min-w-0 flex-1 truncate text-fg-2" dir="auto" title={l.text}>
                    {l.image ? `Image alt: ${l.text}` : l.text}
                  </span>
                  {state.focus === null && (
                    <span className="max-w-2/5 shrink-0 truncate" title={l.to_url}>
                      to {new URL(l.to_url).pathname}
                    </span>
                  )}
                  {l.rel && <span className="shrink-0">{l.rel}</span>}
                </div>
              </li>
            ))}
          </ul>
        )}
        {links && links.links.length < links.total && (
          <div className="py-1.5 text-xs text-fg-3">
            The first {links.links.length.toLocaleString()} of {links.total.toLocaleString()} are listed.
          </div>
        )}
      </div>
    </div>
  );
}
