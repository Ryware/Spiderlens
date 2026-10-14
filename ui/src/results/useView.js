// One open view of the crawl file and its rows (plan D4, D5, D9).
//
// The first screen comes back with the open call. The engine then builds
// the view's full order and says so (`view-ready`); from then on any
// position can be read, a page of rows at a time, and cached here. While a
// crawl runs the view is rebuilt in the background, at most every second
// and never more often than three times what a rebuild costs, and the
// caller is told where its anchor rows moved so the table does not jump.
import { useCallback, useEffect, useRef, useState } from "react";
export const PAGE = 100;
/** Pages kept in memory per view; the farthest from the newest are dropped. */
export const MAX_PAGES = 30;
const closed = { view: null, version: 0, total: 0, ready: false, loading: false, error: null };
export function useView(transport, path, query, opts) {
    const [state, setState] = useState(closed);
    const [, setTick] = useState(0);
    const pages = useRef(new Map());
    const pending = useRef(new Set());
    const current = useRef(closed);
    current.current = state;
    const early = useRef(new Map());
    /** The rows last asked for: what is on screen. */
    const shown = useRef([0, 0]);
    const optsRef = useRef(opts);
    optsRef.current = opts;
    const [retry, setRetry] = useState(0);
    const key = query && JSON.stringify(query);
    useEffect(() => {
        const off = transport.onViewReady((r) => {
            if (r.view === current.current.view)
                setState((s) => ({ ...s, total: r.total, ready: true }));
            else
                early.current.set(r.view, r.total);
        });
        return () => void off.then((o) => o());
    }, [transport]);
    useEffect(() => {
        if (!path || !query) {
            setState(closed);
            return;
        }
        let cancelled = false;
        let opened = null;
        setState((s) => ({ ...s, loading: true, error: null }));
        transport.viewOpen(path, query, PAGE).then((p) => {
            if (cancelled) {
                void transport.viewClose(path, p.view).catch(() => { });
                return;
            }
            opened = p.view;
            pages.current = new Map([[0, p.rows]]);
            pending.current = new Set();
            const readyTotal = early.current.get(p.view);
            early.current.clear();
            const total = p.total ?? readyTotal ?? p.rows.length;
            setState({ view: p.view, version: p.version, total, ready: p.ready || readyTotal !== undefined || p.rows.length < PAGE, loading: false, error: null });
        }, (e) => {
            if (!cancelled)
                setState({ ...closed, error: String(e) });
        });
        return () => {
            cancelled = true;
            if (opened !== null)
                void transport.viewClose(path, opened).catch(() => { });
        };
        // `key` stands for the query's contents.
    }, [transport, path, key, retry]);
    // A new crawl's file may not be readable yet: try again while it runs.
    useEffect(() => {
        if (!state.error || !opts.live)
            return;
        const t = setTimeout(() => setRetry((r) => r + 1), 1000);
        return () => clearTimeout(t);
    }, [state.error, opts.live]);
    const row = useCallback((i) => pages.current.get(Math.floor(i / PAGE))?.[i % PAGE], []);
    /** Loads the pages covering rows [first, last). */
    const ensure = useCallback((first, last) => {
        const s = current.current;
        if (!path || s.view === null || last <= first)
            return;
        shown.current = [first, last];
        const view = s.view;
        const version = s.version;
        for (let p = Math.floor(first / PAGE); p <= Math.floor((last - 1) / PAGE); p++) {
            if (pages.current.has(p) || pending.current.has(p))
                continue;
            if (!s.ready && p > 0)
                continue;
            pending.current.add(p);
            transport.viewRows(path, view, p * PAGE, PAGE).then((r) => {
                const now = current.current;
                if (now.view !== view || now.version !== version || r.version !== version)
                    return;
                pending.current.delete(p);
                pages.current.set(p, r.rows);
                if (pages.current.size > MAX_PAGES) {
                    const far = [...pages.current.keys()].sort((a, b) => Math.abs(b - p) - Math.abs(a - p));
                    for (const k of far.slice(0, pages.current.size - MAX_PAGES))
                        pages.current.delete(k);
                }
                setTick((t) => t + 1);
            }, () => pending.current.delete(p));
        }
    }, 
    // A new version empties the cache, so the table asks again.
    [transport, path, state.version]);
    /** The pages covering what is on screen, shifted by `delta`, read at `version`. */
    const rowsAround = useCallback(async (view, version, total, delta) => {
        const out = new Map();
        const first = Math.max(0, shown.current[0] + delta);
        const last = Math.min(total, shown.current[1] + delta);
        if (!path || last <= first)
            return out;
        const want = [];
        for (let p = Math.floor(first / PAGE); p <= Math.floor((last - 1) / PAGE); p++)
            want.push(p);
        const got = await Promise.all(want.map((p) => transport.viewRows(path, view, p * PAGE, PAGE)));
        got.forEach((g, k) => {
            if (g.version === version)
                out.set(want[k], g.rows);
        });
        return out;
    }, [transport, path]);
    const refresh = useCallback(async () => {
        const s = current.current;
        if (!path || s.view === null || !s.ready)
            return null;
        const view = s.view;
        const anchors = optsRef.current.anchors();
        const r = await transport.viewRefresh(path, view, anchors);
        if (!r || current.current.view !== view)
            return r?.build_ms ?? null;
        // The rows on screen stay until the new version's rows for the same
        // place have arrived, then both swap at once. Dropping the cache first
        // blanked the table every second while a crawl ran.
        const fresh = await rowsAround(view, r.version, r.total, moved(anchors[0], r.positions[0])).catch(() => new Map());
        if (current.current.view !== view || current.current.version !== s.version)
            return r.build_ms;
        pages.current = fresh;
        pending.current = new Set();
        setState((x) => ({ ...x, version: r.version, total: r.total }));
        optsRef.current.onRefresh(r.positions, r.total);
        return r.build_ms;
        /** How far the top anchor moved, so the pages fetched are the ones that will be on screen. */
        function moved(id, to) {
            if (id === undefined || id < 0 || to == null)
                return 0;
            for (const [p, rows] of pages.current) {
                const i = rows.findIndex((x) => x[0] === id);
                if (i >= 0)
                    return to - (p * PAGE + i);
            }
            return 0;
        }
    }, [transport, path, rowsAround]);
    // Live rebuilds while the crawl runs, then one more when it ends so the
    // last rows written show up.
    const live = opts.live && state.ready && state.view !== null;
    const wasLive = useRef(false);
    useEffect(() => {
        if (!live) {
            if (wasLive.current)
                void refresh();
            wasLive.current = false;
            return;
        }
        wasLive.current = true;
        let stop = false;
        let timer;
        const loop = (delay) => {
            timer = setTimeout(async () => {
                const ms = await refresh().catch(() => null);
                if (!stop)
                    loop(Math.max(1000, 3 * (ms ?? 0)));
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
