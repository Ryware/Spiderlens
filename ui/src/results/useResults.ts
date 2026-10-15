import { useCallback, useEffect, useState } from "react";
import type { Features } from "../generated/Features";
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
 */
export function useResults(transport: Transport, path: string | null, live: boolean) {
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

  const session = useCallback((tab: string) => sessions[tab] ?? newSession(), [sessions]);
  const update = useCallback((tab: string, patch: Partial<TabSession>) => setSessions((s) => ({ ...s, [tab]: { ...(s[tab] ?? newSession()), ...patch } })), []);

  const counts = useTabCounts(transport, features ? path : null, live);

  return { tabs, features, session, update, focusId, setFocusId, counts };
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
