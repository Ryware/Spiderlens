// One open view of the crawl file and its rows (plan D4, D5, D9).
//
// The first screen comes back with the open call. The engine then builds
// the view's full order and says so (`view-ready`); from then on any
// position can be read, a page of rows at a time, and cached here. While a
// crawl runs the view is rebuilt in the background, at most every second
// and never more often than three times what a rebuild costs, and the
// caller is told where its anchor rows moved so the table does not jump.

import { useCallback, useEffect, useRef, useState } from "react";
import type { JsonValue } from "../generated/serde_json/JsonValue";
import type { ViewQuery } from "../generated/ViewQuery";
import type { Transport } from "../transport";

export const PAGE = 100;

export type Row = JsonValue[];

export interface ViewState {
  view: number | null;
  version: number;
  /** Rows in the view; only the first page is known until `ready`. */
  total: number;
  ready: boolean;
  loading: boolean;
  error: string | null;
}

const closed: ViewState = { view: null, version: 0, total: 0, ready: false, loading: false, error: null };

export interface ViewOptions {
  /** Rebuild as the crawl writes. */
  live: boolean;
  /** Ids whose new positions the caller needs after a rebuild. */
  anchors: () => number[];
  onRefresh: (positions: (number | null)[], total: number) => void;
}

export function useView(transport: Transport, path: string | null, query: ViewQuery | null, opts: ViewOptions) {
  const [state, setState] = useState<ViewState>(closed);
  const [, setTick] = useState(0);
  const pages = useRef(new Map<number, Row[]>());
  const pending = useRef(new Set<number>());
  const current = useRef<ViewState>(closed);
  current.current = state;
  const early = useRef(new Map<number, number>());
  const optsRef = useRef(opts);
  optsRef.current = opts;
  const [retry, setRetry] = useState(0);
  const key = query && JSON.stringify(query);

  useEffect(() => {
    const off = transport.onViewReady((r) => {
      if (r.view === current.current.view) setState((s) => ({ ...s, total: r.total, ready: true }));
      else early.current.set(r.view, r.total);
    });
    return () => void off.then((o) => o());
  }, [transport]);

  useEffect(() => {
    if (!path || !query) {
      setState(closed);
      return;
    }
    let cancelled = false;
    let opened: number | null = null;
    setState((s) => ({ ...s, loading: true, error: null }));
    transport.viewOpen(path, query, PAGE).then(
      (p) => {
        if (cancelled) {
          void transport.viewClose(path, p.view).catch(() => {});
          return;
        }
        opened = p.view;
        pages.current = new Map([[0, p.rows]]);
        pending.current = new Set();
        const readyTotal = early.current.get(p.view);
        early.current.clear();
        const total = p.total ?? readyTotal ?? p.rows.length;
        setState({ view: p.view, version: p.version, total, ready: p.ready || readyTotal !== undefined || p.rows.length < PAGE, loading: false, error: null });
      },
      (e) => {
        if (!cancelled) setState({ ...closed, error: String(e) });
      },
    );
    return () => {
      cancelled = true;
      if (opened !== null) void transport.viewClose(path, opened).catch(() => {});
    };
    // `key` stands for the query's contents.
  }, [transport, path, key, retry]);

  // A new crawl's file may not be readable yet: try again while it runs.
  useEffect(() => {
    if (!state.error || !opts.live) return;
    const t = setTimeout(() => setRetry((r) => r + 1), 1000);
    return () => clearTimeout(t);
  }, [state.error, opts.live]);

  const row = useCallback((i: number): Row | undefined => pages.current.get(Math.floor(i / PAGE))?.[i % PAGE], []);

  /** Loads the pages covering rows [first, last). */
  const ensure = useCallback(
    (first: number, last: number) => {
      const s = current.current;
      if (!path || s.view === null || last <= first) return;
      const view = s.view;
      const version = s.version;
      for (let p = Math.floor(first / PAGE); p <= Math.floor((last - 1) / PAGE); p++) {
        if (pages.current.has(p) || pending.current.has(p)) continue;
        if (!s.ready && p > 0) continue;
        pending.current.add(p);
        transport.viewRows(path, view, p * PAGE, PAGE).then(
          (r) => {
            const now = current.current;
            if (now.view !== view || now.version !== version || r.version !== version) return;
            pending.current.delete(p);
            pages.current.set(p, r.rows);
            setTick((t) => t + 1);
          },
          () => pending.current.delete(p),
        );
      }
    },
    [transport, path],
  );

  const refresh = useCallback(async (): Promise<number | null> => {
    const s = current.current;
    if (!path || s.view === null || !s.ready) return null;
    const view = s.view;
    const r = await transport.viewRefresh(path, view, optsRef.current.anchors());
    if (!r || current.current.view !== view) return r?.build_ms ?? null;
    pages.current = new Map();
    pending.current = new Set();
    setState((x) => ({ ...x, version: r.version, total: r.total }));
    optsRef.current.onRefresh(r.positions, r.total);
    return r.build_ms;
  }, [transport, path]);

  // Live rebuilds while the crawl runs, then one more when it ends so the
  // last rows written show up.
  const live = opts.live && state.ready && state.view !== null;
  const wasLive = useRef(false);
  useEffect(() => {
    if (!live) {
      if (wasLive.current) void refresh();
      wasLive.current = false;
      return;
    }
    wasLive.current = true;
    let stop = false;
    let timer: ReturnType<typeof setTimeout>;
    const loop = (delay: number) => {
      timer = setTimeout(async () => {
        const ms = await refresh().catch(() => null);
        if (!stop) loop(Math.max(1000, 3 * (ms ?? 0)));
      }, delay);
    };
    loop(1000);
    return () => {
      stop = true;
      clearTimeout(timer);
    };
  }, [live, refresh]);

  return { ...state, row, ensure, refresh };
}

export type View = ReturnType<typeof useView>;
