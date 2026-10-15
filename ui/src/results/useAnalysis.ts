import { useCallback, useEffect, useRef, useState } from "react";
import type { AnalysisProgress } from "../generated/AnalysisProgress";
import type { AnalysisRun } from "../generated/AnalysisRun";
import type { Phase } from "../crawl/useCrawlRun";
import type { Transport } from "../transport";

export type AnalysisStatus = "idle" | "running" | "done" | "cancelled" | "failed";

export interface Analysis {
  status: AnalysisStatus;
  progress: AnalysisProgress | null;
  run: AnalysisRun | null;
  error: string | null;
  start(): void;
  cancel(): void;
}

type State = Pick<Analysis, "status" | "progress" | "run" | "error">;
const idle: State = { status: "idle", progress: null, run: null, error: null };

/**
 * Crawl Analysis for the open crawl file (spec 023 FR-012, FR-013): runs on
 * its own when a crawl finishes if `auto` is on, reports progress, and can
 * be cancelled. It runs after fetching, never during it.
 */
export function useAnalysis(transport: Transport, path: string | null, phase: Phase, running: boolean, auto: boolean): Analysis {
  const [state, setState] = useState<State>(idle);
  const pathRef = useRef(path);
  pathRef.current = path;

  useEffect(() => setState(idle), [path]);

  useEffect(() => {
    const offs = [
      transport.onAnalysisProgress((p) => {
        if (p.path !== pathRef.current) return;
        setState((s) => (s.status === "running" ? { ...s, progress: { phase: p.phase, done: p.done, total: p.total } } : s));
      }),
      transport.onAnalysisDone((d) => {
        if (d.path !== pathRef.current) return;
        setState({ status: d.error ? "failed" : d.cancelled ? "cancelled" : "done", progress: null, run: d.run, error: d.error });
      }),
    ];
    return () => offs.forEach((o) => o.then((off) => off()));
  }, [transport]);

  const start = useCallback(() => {
    if (!path || running) return;
    setState({ ...idle, status: "running" });
    transport.analysisStart(path).catch((e) => setState({ ...idle, status: "failed", error: String(e) }));
  }, [transport, path, running]);

  const cancel = useCallback(() => {
    void transport.analysisCancel().catch(() => {});
  }, [transport]);

  // When a crawl finishes (not when one is paused, stopped or opened).
  const wasRunning = useRef(running);
  useEffect(() => {
    const was = wasRunning.current;
    wasRunning.current = running;
    if (was && !running && phase === "completed" && auto) start();
  }, [running, phase, auto, start]);

  return { ...state, start, cancel };
}

/** The status bar line for a running or finished analysis. */
export function analysisLabel(a: Pick<Analysis, "status" | "progress">): string | null {
  switch (a.status) {
    case "running": {
      const p = a.progress;
      const phase = p?.phase === "link_score" ? "Link Score" : "Link positions";
      const pct = p && p.total > 0 ? ` ${Math.floor((p.done * 100) / p.total)}%` : "";
      return `Analysing: ${phase}${pct}`;
    }
    case "done":
      return "Link Score ready";
    case "cancelled":
      return "Analysis cancelled";
    case "failed":
      return "Analysis failed";
    default:
      return null;
  }
}
