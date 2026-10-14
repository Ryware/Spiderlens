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
  /** Bumped when custom rules ran again on this file, so open views reload (spec 013 US3). */
  const [epoch, setEpoch] = useState(0);

  // The open file's tabs include its custom extraction and search tabs.
  // A finished crawl is read again: its rules' tabs appear once pages exist.
  useEffect(() => {
    let off = false;
    const show = (t: TabDef[]) => {
      if (!off) setTabs(t);
    };
    // A new crawl's file can lag its start; the built-in tabs show meanwhile.
    transport.resultsTabs(path ?? undefined).then(show, () => transport.resultsTabs().then(show, () => {}));
    return () => {
      off = true;
    };
  }, [transport, path, live, epoch]);

  useEffect(() => {
    const off = transport.onCustomRunDone((d) => {
      if (d.path === path && d.state !== "failed") setEpoch((n) => n + 1);
    });
    return () => void off.then((f) => f());
  }, [transport, path]);

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

  return { tabs, features, session, update, focusId, setFocusId, epoch };
}

export type Results = ReturnType<typeof useResults>;
