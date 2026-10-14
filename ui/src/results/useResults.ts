import { useCallback, useEffect, useRef, useState } from "react";
import type { Features } from "../generated/Features";
import type { Segment } from "../generated/Segment";
import type { SegmentCounts } from "../generated/SegmentCounts";
import type { Sort } from "../generated/Sort";
import type { TabDef } from "../generated/TabDef";
import type { UrlScope } from "../generated/UrlScope";
import type { Transport } from "../transport";

/** What each tab remembers while the app is open (FR-017). */
export interface TabSession {
  filter: string;
  scope: UrlScope;
  search: string;
  sort: Sort | null;
  focusId: number | null;
}

export const newSession = (): TabSession => ({ filter: "all", scope: "all", search: "", sort: null, focusId: null });

/**
 * The open crawl file's results: tab definitions (from the engine, D2),
 * which data the file has, and each tab's session. Reset when the file
 * changes.
 *
 * Segments (spec 027 part B): the settings' segments group the open
 * crawl's URLs whenever they change and are valid; until the settings have
 * any, the crawl keeps the segments it was made with. The segment filter is
 * one choice for every tab.
 */
export function useResults(transport: Transport, path: string | null, live: boolean, segments?: Segment[], segmentsValid = true) {
  const [tabs, setTabs] = useState<TabDef[]>([]);
  const [features, setFeatures] = useState<Features | null>(null);
  const [sessions, setSessions] = useState<Record<string, TabSession>>({});
  const [focusId, setFocusId] = useState<number | null>(null);
  const [retry, setRetry] = useState(0);

  useEffect(() => {
    transport.resultsTabs().then(setTabs, () => {});
  }, [transport]);

  useEffect(() => {
    setSessions({});
    setFocusId(null);
    setFeatures(null);
  }, [path]);

  useEffect(() => {
    if (!path) return;
    let off = false;
    transport.resultsOpen(path).then(
      (r) => !off && setFeatures(r.features),
      // A new crawl's file can lag its start by a moment.
      () => {
        if (!off && live) setTimeout(() => setRetry((n) => n + 1), 1000);
      },
    );
    return () => {
      off = true;
    };
  }, [transport, path, live, retry]);

  // The AI checks add data to the open file (spec 015): read the list again.
  useEffect(() => {
    if (!path) return;
    const off = transport.onAeoFinished((f) => {
      if (f.path === path && f.summary) setRetry((n) => n + 1);
    });
    return () => void off.then((f) => f());
  }, [transport, path]);
  const [segment, setSegment] = useState<number | null>(null);
  const [segmentCounts, setSegmentCounts] = useState<SegmentCounts | null>(null);
  /** Bumped when the segments change, so views and counts reload. */
  const [segmentsRev, setSegmentsRev] = useState(0);
  const applied = useRef(false);
  /** The segments the open crawl is grouped by, as JSON. */
  const grouping = useRef<string | null>(null);
  useEffect(() => {
    setSegment(null);
    setSegmentCounts(null);
    applied.current = false;
    grouping.current = null;
  }, [path]);
  const segmentsKey = segments ? JSON.stringify(segments) : null;
  const opened = features !== null;
  useEffect(() => {
    if (!path || !opened || !segments || !segmentsValid) return;
    let off = false;
    const t = setTimeout(() => {
      const keep = !applied.current && segments.length === 0;
      (keep ? transport.segmentsCounts(path) : transport.segmentsSet(path, segments)).then(
        (c) => {
          if (off) return;
          applied.current = applied.current || !keep;
          setSegmentCounts(c);
          // Views reload only when the grouping really changed.
          const now = JSON.stringify(c.segments);
          const changed = grouping.current === null ? !keep && c.segments.length > 0 : grouping.current !== now;
          if (changed) setSegmentsRev((n) => n + 1);
          grouping.current = now;
          setSegment((v) => (v === null || v === 0 || c.segments.some((s) => s.enabled && s.id === v) ? v : null));
        },
        () => {},
      );
    }, 300);
    return () => {
      off = true;
      clearTimeout(t);
    };
    // segmentsKey stands for the segments' contents; `live` ending saves
    // them into the file, which the running crawl held.
  }, [transport, path, opened, segmentsKey, segmentsValid, live]);
  const refreshSegmentCounts = useCallback(() => {
    if (!path || !segmentCounts?.segments.some((s) => s.enabled)) return;
    transport.segmentsCounts(path).then(setSegmentCounts, () => {});
  }, [transport, path, segmentCounts]);

  const session = useCallback((tab: string) => sessions[tab] ?? newSession(), [sessions]);
  const update = useCallback((tab: string, patch: Partial<TabSession>) => setSessions((s) => ({ ...s, [tab]: { ...(s[tab] ?? newSession()), ...patch } })), []);

  const counts = useTabCounts(transport, features ? path : null, live);

  return { tabs, features, session, update, focusId, setFocusId, counts, segment, setSegment, segmentCounts, segmentsRev, refreshSegmentCounts };
}

/**
 * URLs per result tab for the sidebar (T014). Asked once when a file
 * opens, then on its own timer while the crawl runs: each answer is one
 * query, never two at once, and never in the way of the table's refresh.
 * Null while unknown.
 */
export function useTabCounts(transport: Transport, path: string | null, live: boolean) {
  const [counts, setCounts] = useState<Record<string, number> | null>(null);
  useEffect(() => setCounts(null), [path]);
  useEffect(() => {
    if (!path) return;
    let stop = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const ask = async () => {
      const t = Date.now();
      const c = await transport.tabCounts(path).catch(() => null);
      if (stop) return;
      if (c) setCounts(c.counts);
      // Slow files are asked less often, like the table's own refresh.
      if (live) timer = setTimeout(ask, Math.max(1000, 3 * (Date.now() - t)));
    };
    // Also asked once more when the crawl ends, so the last rows are counted.
    void ask();
    return () => {
      stop = true;
      clearTimeout(timer);
    };
  }, [transport, path, live]);
  return counts;
}

export type Results = ReturnType<typeof useResults>;
