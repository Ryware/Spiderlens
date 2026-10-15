import { useCallback, useEffect, useRef, useState } from "react";
import { busyPhases, type CrawlRun } from "../crawl/useCrawlRun";
import type { CrawlInfo, OpenFailure, ResumeOptions, Transport } from "../transport";
import { fileStem, formatDateTime, isCrawlFile } from "./format";

/** The one question or message the crawl file flow is waiting on. */
export type FilePrompt =
  | { kind: "pause_first"; path: string }
  | { kind: "damaged"; path: string; failure: OpenFailure }
  | { kind: "upgrade"; info: CrawlInfo }
  | { kind: "read_only"; info: CrawlInfo }
  | { kind: "secrets"; info: CrawlInfo; upgrade?: ResumeOptions["upgrade"] }
  | { kind: "message"; title: string; body: string };

/** crawlOpen rejects with an OpenFailure as JSON; anything else is shown as it is. */
export function parseFailure(e: unknown): OpenFailure {
  const raw = typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
  try {
    const v = JSON.parse(raw);
    if (v && typeof v.message === "string") return { message: v.message, salvageable: !!v.salvageable };
  } catch {
    // Not JSON: a plain message.
  }
  return { message: raw, salvageable: false };
}

const FLASH_MS = 4000;

/**
 * Opening, resuming and saving crawl files (spec 006 US2, US3, US6). Every
 * way in (Open dialog, recent list, a file dropped on the window, the OS
 * opening a file) goes through `open`, so a running crawl is never
 * interrupted without asking and damaged files are explained the same way.
 */
export function useCrawlFile(
  transport: Transport,
  crawl: CrawlRun,
  { onOpened, onChanged }: { onOpened?: (path: string) => void; onChanged?: () => void } = {},
) {
  const [prompt, setPrompt] = useState<FilePrompt | null>(null);
  const [flash, setFlash] = useState<string | null>(null);
  const [quitting, setQuitting] = useState(false);
  // Opened once the running crawl has paused.
  const [afterPause, setAfterPause] = useState<string | null>(null);
  // Listeners subscribe once; they reach the current crawl and callbacks here.
  const latest = useRef({ crawl, onOpened, onChanged });
  latest.current = { crawl, onOpened, onChanged };

  const message = useCallback((title: string, body: string) => setPrompt({ kind: "message", title, body }), []);

  const show = useCallback(async (path: string) => {
    let info: CrawlInfo;
    try {
      info = await transport.crawlOpen(path);
    } catch (e) {
      setPrompt({ kind: "damaged", path, failure: parseFailure(e) });
      return false;
    }
    const ok = await latest.current.crawl.open(info.path);
    latest.current.onChanged?.();
    if (ok) latest.current.onOpened?.(info.path);
    // Passes missing or stale on this file run now (spec 009 plan D12); a
    // file this session cannot write is shown without them.
    if (ok && info.access.kind === "writable") void transport.passesRun(info.path).catch(() => {});
    return ok;
  }, [transport]);

  const open = useCallback(
    async (path: string) => {
      if (busyPhases.includes(latest.current.crawl.phase)) {
        setPrompt({ kind: "pause_first", path });
        return false;
      }
      return show(path);
    },
    [show],
  );
  const openRef = useRef(open);
  openRef.current = open;

  const confirmPause = useCallback(() => {
    if (prompt?.kind !== "pause_first") return;
    setAfterPause(prompt.path);
    setPrompt(null);
  }, [prompt]);

  // Pause the running crawl, then open the file once it has settled.
  useEffect(() => {
    if (!afterPause) return;
    if (crawl.phase === "running") latest.current.crawl.pause();
    else if (!busyPhases.includes(crawl.phase)) {
      setAfterPause(null);
      void show(afterPause);
    }
  }, [afterPause, crawl.phase, show]);

  const openPicker = useCallback(async () => {
    const path = await transport.pickCrawlFile();
    if (path) await open(path);
  }, [transport, open]);

  const recover = useCallback(async () => {
    if (prompt?.kind !== "damaged") return;
    setPrompt(null);
    try {
      const info = await transport.crawlSalvage(prompt.path);
      await open(info.path);
    } catch (e) {
      message("Nothing could be recovered", parseFailure(e).message);
    }
  }, [transport, prompt, open, message]);

  /**
   * Resume with the checks a crawl file needs first: an older format is
   * upgraded only after asking, secrets are entered again, a read-only file
   * is saved elsewhere first, and a file in use or from a newer version is
   * explained instead.
   */
  const resume = useCallback(
    async (path: string | null = latest.current.crawl.path) => {
      if (!path) return;
      let info: CrawlInfo;
      try {
        info = await transport.crawlInfo(path);
      } catch {
        // The engine says why when it refuses.
        await latest.current.crawl.resume(path);
        return;
      }
      const a = info.access;
      switch (a.kind) {
        case "writable":
          if (info.missing_secrets.length) setPrompt({ kind: "secrets", info });
          else await latest.current.crawl.resume(path);
          return;
        case "needs_upgrade":
          setPrompt({ kind: "upgrade", info });
          return;
        case "read_only_media":
          setPrompt({ kind: "read_only", info });
          return;
        case "in_use":
          message("This crawl is open somewhere else", `SpiderLens ${a.by.app_version} on ${a.by.host} has had it open since ${formatDateTime(a.by.since)}. Close it there to resume it here.`);
          return;
        case "newer_format":
          message("Made by a newer SpiderLens", `Resuming this crawl needs SpiderLens ${a.needs} or later. Its results can still be viewed here.`);
          return;
        case "view_only":
          message("This crawl can only be viewed", a.reason);
          return;
      }
    },
    [transport, message],
  );

  const confirmUpgrade = useCallback(
    async (keepCopy: boolean) => {
      if (prompt?.kind !== "upgrade") return;
      const upgrade = keepCopy ? "keep_copy" : "no_copy";
      if (prompt.info.missing_secrets.length) {
        setPrompt({ kind: "secrets", info: prompt.info, upgrade });
        return;
      }
      setPrompt(null);
      await latest.current.crawl.resume(prompt.info.path, { upgrade });
    },
    [prompt],
  );

  const submitSecrets = useCallback(
    async (secrets: Record<string, string>) => {
      if (prompt?.kind !== "secrets") return;
      setPrompt(null);
      await latest.current.crawl.resume(prompt.info.path, prompt.upgrade ? { secrets, upgrade: prompt.upgrade } : { secrets });
    },
    [prompt],
  );

  const showFlash = useCallback((text: string) => setFlash(text), []);
  useEffect(() => {
    if (!flash) return;
    const t = setTimeout(() => setFlash(null), FLASH_MS);
    return () => clearTimeout(t);
  }, [flash]);

  /** Saves the crawl at `path` somewhere the user picks; returns the new path. */
  const saveAs = useCallback(
    async (path: string | null = latest.current.crawl.path) => {
      if (!path) return null;
      const dest = await transport.pickSavePath(fileStem(path));
      if (!dest) return null;
      try {
        const saved = await transport.crawlSaveAs(path, dest);
        if (latest.current.crawl.path === path) latest.current.crawl.moved(saved);
        showFlash(`Saved to ${saved}`);
        latest.current.onChanged?.();
        return saved;
      } catch (e) {
        message("Couldn't save the crawl", parseFailure(e).message);
        return null;
      }
    },
    [transport, message, showFlash],
  );

  /** A read-only file: save a copy somewhere writable, then resume that. */
  const saveCopyAndResume = useCallback(async () => {
    if (prompt?.kind !== "read_only") return;
    const saved = await saveAs(prompt.info.path);
    if (!saved) return;
    setPrompt((p) => (p?.kind === "read_only" ? null : p));
    await resume(saved);
  }, [prompt, saveAs, resume]);

  useEffect(() => {
    const offs = [
      transport.onOpenFile((path) => void openRef.current(path)),
      // Only crawl files are taken; anything else dropped on the window is ignored.
      transport.onFileDrop((paths) => {
        const path = paths.find(isCrawlFile);
        if (path) void openRef.current(path);
      }),
      transport.onAppQuitting((q) => q.pausing && setQuitting(true)),
    ];
    // Files the OS asked us to open before the window was ready.
    let live = true;
    transport.pendingOpen().then(
      (paths) => {
        const path = paths.find(isCrawlFile) ?? paths[0];
        if (live && path) void openRef.current(path);
      },
      () => {},
    );
    return () => {
      live = false;
      offs.forEach((o) => o.then((off) => off()));
    };
  }, [transport]);

  return {
    prompt,
    dismiss: () => setPrompt(null),
    flash,
    quitting,
    waitingToOpen: afterPause,
    open,
    openPicker,
    confirmPause,
    recover,
    resume,
    confirmUpgrade,
    submitSecrets,
    saveAs,
    saveCopyAndResume,
  };
}

export type CrawlFile = ReturnType<typeof useCrawlFile>;
