import { useCallback, useEffect, useRef, useState } from "react";
import type { RecentEntry, StorageStatus, Transport, UnsavedCrawls } from "../transport";

const byLastUsed = (a: RecentEntry, b: RecentEntry) => (Date.parse(b.last_used) || 0) - (Date.parse(a.last_used) || 0);

/**
 * What the start screen lists (spec 006 US4): recent crawls, crawls not yet
 * saved to a chosen place and the space left for them. Reloads when crawls
 * are recovered at start-up and whenever a crawl finishes.
 */
export function useCrawlLibrary(transport: Transport) {
  const [recent, setRecent] = useState<RecentEntry[]>([]);
  const [unsaved, setUnsaved] = useState<UnsavedCrawls | null>(null);
  const [storage, setStorage] = useState<StorageStatus | null>(null);

  // Only changes are set, so a reload that finds nothing new does not redraw the start screen.
  const last = useRef(new Map<string, string>());
  const changed = useCallback((key: string, value: unknown) => {
    const json = JSON.stringify(value);
    if (last.current.get(key) === json) return false;
    last.current.set(key, json);
    return true;
  }, []);

  const reload = useCallback(() => {
    transport.recentList().then(
      (r) => {
        const sorted = [...r].sort(byLastUsed);
        if (changed("recent", sorted)) setRecent(sorted);
      },
      () => changed("recent", []) && setRecent([]),
    );
    transport.unsavedList().then(
      (u) => changed("unsaved", u) && setUnsaved(u),
      () => changed("unsaved", null) && setUnsaved(null),
    );
    transport.storageStatus().then(
      (st) => changed("storage", st) && setStorage(st),
      () => changed("storage", null) && setStorage(null),
    );
  }, [transport, changed]);

  useEffect(() => {
    reload();
    const offs = [transport.onCrawlsRecovered(() => reload()), transport.onCrawlFinished(() => reload())];
    return () => offs.forEach((o) => o.then((off) => off()));
  }, [transport, reload]);

  /** Takes the entry off the list; the file itself stays where it is. */
  const remove = useCallback(
    async (path: string) => {
      await transport.recentRemove(path);
      reload();
    },
    [transport, reload],
  );

  /** Points a "File not found" entry at the file's new place. */
  const locate = useCallback(
    async (path: string) => {
      const found = await transport.pickCrawlFile();
      if (!found) return;
      await transport.recentLocate(path, found);
      reload();
    },
    [transport, reload],
  );

  const deleteUnsaved = useCallback(
    async (path: string) => {
      await transport.unsavedDelete(path);
      reload();
    },
    [transport, reload],
  );

  return { recent, unsaved, storage, reload, remove, locate, deleteUnsaved };
}

export type CrawlLibrary = ReturnType<typeof useCrawlLibrary>;
