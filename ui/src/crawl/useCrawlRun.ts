import { useCallback, useEffect, useRef, useState } from "react";
import type { CrawlConfig } from "../generated/CrawlConfig";
import type { CrawlFinished, CrawlProgress, CrawlReport, Transport } from "../transport";

/**
 * Where the crawl is in its life. "pausing" and "stopping" last while the
 * requests in flight finish (spec 003 FR-022).
 */
export type Phase = "idle" | "starting" | "running" | "pausing" | "stopping" | "paused" | "stopped" | "completed" | "failed";

/** The crawl owns the window in these phases: settings are locked. */
export const activePhases: Phase[] = ["starting", "running", "pausing", "stopping", "paused"];

/** URLs per second is averaged over this window, so the figure does not jitter. */
export const RATE_WINDOW_MS = 5000;

interface Sample {
  t: number;
  crawled: number;
}

export interface CrawlRunState {
  phase: Phase;
  id: number | null;
  /** The crawl file. Saved as the crawl runs. */
  path: string | null;
  /** What is being crawled, for the status line: a host or "N URLs". */
  target: string;
  progress: CrawlProgress | null;
  report: CrawlReport | null;
  error: string | null;
  warnings: string[];
}

const initial: CrawlRunState = { phase: "idle", id: null, path: null, target: "", progress: null, report: null, error: null, warnings: [] };

const phaseFor = (f: CrawlFinished): Phase => (f.error ? "failed" : (f.report?.summary.status as Phase | undefined) ?? "completed");

/**
 * One crawl at a time: start, pause, resume, stop, and the live figures the
 * engine sends. Time and speed are measured here; everything else comes from
 * the engine.
 */
export function useCrawlRun(transport: Transport) {
  const [state, setState] = useState<CrawlRunState>(initial);
  const [now, setNow] = useState(() => Date.now());
  // Time spent crawling before the current run segment (pauses excluded).
  const elapsedBefore = useRef(0);
  const segmentStart = useRef<number | null>(null);
  const samples = useRef<Sample[]>([]);
  // When the last progress event arrived: the engine's active time is
  // counted on from there between events.
  const progressAt = useRef(0);
  const idRef = useRef<number | null>(null);
  // A crawl that fails at once can finish before its start call returns.
  const early = useRef(new Map<number, CrawlFinished>());

  const endSegment = () => {
    if (segmentStart.current !== null) elapsedBefore.current += Date.now() - segmentStart.current;
    segmentStart.current = null;
    samples.current = [];
  };

  const finish = useCallback((f: CrawlFinished) => {
    endSegment();
    idRef.current = null;
    setState((s) => ({ ...s, phase: phaseFor(f), id: null, path: f.path, report: f.report, error: f.error }));
  }, []);

  useEffect(() => {
    const offs = [
      transport.onCrawlProgress((p) => {
        if (p.id !== idRef.current) return;
        const t = Date.now();
        samples.current = [...samples.current.filter((x) => t - x.t <= RATE_WINDOW_MS), { t, crawled: p.crawled }];
        progressAt.current = t;
        setState((s) => ({ ...s, progress: p }));
      }),
      transport.onCrawlWarning((w) => {
        if (w.id !== idRef.current) return;
        setState((s) => (s.warnings.includes(w.message) ? s : { ...s, warnings: [...s.warnings, w.message] }));
      }),
      transport.onCrawlFinished((f) => {
        if (f.id === idRef.current) finish(f);
        else if (idRef.current === null) early.current.set(f.id, f);
      }),
    ];
    return () => offs.forEach((o) => o.then((off) => off()));
  }, [transport, finish]);

  const running = state.phase === "running" || state.phase === "pausing" || state.phase === "stopping";
  useEffect(() => {
    if (!running) return;
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [running]);

  const begin = useCallback(
    async (call: () => Promise<{ id: number; path: string }>, reset: Partial<CrawlRunState>) => {
      setState((s) => ({ ...s, ...reset, phase: "starting", error: null }));
      try {
        const started = await call();
        const done = early.current.get(started.id);
        early.current.clear();
        segmentStart.current = Date.now();
        setNow(Date.now());
        idRef.current = started.id;
        setState((s) => ({ ...s, phase: "running", id: started.id, path: started.path }));
        if (done) finish(done);
        return started.id;
      } catch (e) {
        setState((s) => ({ ...s, phase: "failed", error: String(e) }));
        return null;
      }
    },
    [finish],
  );

  const start = useCallback(
    (config: CrawlConfig, startUrl: string | null, target: string) => {
      elapsedBefore.current = 0;
      return begin(() => transport.crawlStart(config, startUrl), { ...initial, target });
    },
    [transport, begin],
  );

  const resume = useCallback(() => {
    if (!state.path) return Promise.resolve(null);
    const path = state.path;
    return begin(() => transport.crawlResume(path), { report: null });
  }, [transport, begin, state.path]);

  /** Opens a saved crawl file to browse its results (spec 007 D14). */
  const open = useCallback(
    async (path: string) => {
      if (activePhases.includes(state.phase)) return false;
      try {
        const report = await transport.crawlSummary(path);
        const status = report.summary.status;
        elapsedBefore.current = report.summary.active_ms ?? 0;
        segmentStart.current = null;
        samples.current = [];
        setState({
          ...initial,
          phase: status === "running" ? "stopped" : (status as Phase),
          path,
          report,
          target: report.summary.hosts[0]?.host ?? path.split(/[\\/]/).pop() ?? path,
        });
        return true;
      } catch (e) {
        setState((s) => ({ ...s, error: `Could not open ${path}: ${String(e)}` }));
        return false;
      }
    },
    [transport, state.phase],
  );

  const pause = useCallback(() => {
    if (state.id === null || state.phase !== "running") return;
    setState((s) => ({ ...s, phase: "pausing" }));
    transport.crawlPause(state.id);
  }, [transport, state.id, state.phase]);

  const stop = useCallback(async () => {
    if (state.phase === "paused") {
      // A paused crawl has no run to signal: reopen it and stop at once, so
      // its queue is recorded as "Crawl stopped" like any other stop.
      const id = await resume();
      if (id !== null) {
        setState((s) => ({ ...s, phase: "stopping" }));
        transport.crawlStop(id);
      }
      return;
    }
    if (state.id === null || (state.phase !== "running" && state.phase !== "pausing")) return;
    setState((s) => ({ ...s, phase: "stopping" }));
    transport.crawlStop(state.id);
  }, [transport, state.id, state.phase, resume]);

  // The engine's own active time once it reports one (it survives restarts
  // of the app); before that, time measured here.
  const p = state.progress;
  const fallback = elapsedBefore.current + (segmentStart.current !== null ? Math.max(0, now - segmentStart.current) : 0);
  const elapsedMs = running
    ? p?.active_ms != null
      ? p.active_ms + Math.max(0, now - progressAt.current)
      : fallback
    : (state.report?.summary.active_ms ?? p?.active_ms ?? fallback);
  const w = samples.current;
  const rate =
    running && w.length >= 2 && w[w.length - 1].t > w[0].t
      ? ((w[w.length - 1].crawled - w[0].crawled) * 1000) / (w[w.length - 1].t - w[0].t)
      : null;

  return {
    ...state,
    active: activePhases.includes(state.phase),
    running,
    elapsedMs,
    rate,
    start,
    open,
    pause,
    resume,
    stop,
  };
}

export type CrawlRun = ReturnType<typeof useCrawlRun>;

/** 00:42, 12:05, 1:02:09. */
export function formatElapsed(ms: number): string {
  const s = Math.floor(ms / 1000);
  const [h, m, sec] = [Math.floor(s / 3600), Math.floor((s % 3600) / 60), s % 60];
  const two = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${two(m)}:${two(sec)}` : `${two(m)}:${two(sec)}`;
}
