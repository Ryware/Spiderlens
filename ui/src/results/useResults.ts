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

  const session = useCallback((tab: string) => sessions[tab] ?? newSession(), [sessions]);
  const update = useCallback((tab: string, patch: Partial<TabSession>) => setSessions((s) => ({ ...s, [tab]: { ...(s[tab] ?? newSession()), ...patch } })), []);

  return { tabs, features, session, update, focusId, setFocusId };
}

export type Results = ReturnType<typeof useResults>;
