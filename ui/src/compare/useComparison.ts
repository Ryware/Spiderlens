import { useCallback, useEffect, useRef, useState } from "react";
import type { CompareSettings } from "../generated/CompareSettings";
import type { ElementSetting } from "../generated/ElementSetting";
import type { Summary } from "../generated/Summary";
import type { CompareProgress, Transport } from "../transport";

export type ComparisonState =
  | { phase: "idle" }
  | { phase: "running"; id: number; previous: string; current: string; progress: CompareProgress | null }
  | { phase: "ready"; id: number; summary: Summary; settings: CompareSettings }
  | { phase: "failed"; error: string };

/** One comparison at a time (spec 012): start, progress, summary, close. */
export function useComparison(transport: Transport) {
  const [state, setState] = useState<ComparisonState>({ phase: "idle" });
  const idRef = useRef<number | null>(null);
  const settingsRef = useRef<CompareSettings | null>(null);

  useEffect(() => {
    const off = transport.onCompareProgress((p) => {
      if (p.comparison !== idRef.current) return;
      if (p.phase === "done") {
        transport.compareSummary(p.comparison).then(
          (summary) =>
            idRef.current === p.comparison &&
            setState({ phase: "ready", id: p.comparison, summary, settings: settingsRef.current ?? { mapping: [], elements: [], issue_settings: "current" } }),
          (e) => setState({ phase: "failed", error: String(e) }),
        );
      } else if (p.phase === "failed") {
        idRef.current = null;
        setState({ phase: "failed", error: p.error ?? "The comparison failed." });
      } else if (p.phase === "cancelled") {
        idRef.current = null;
        setState({ phase: "idle" });
      } else {
        setState((s) => (s.phase === "running" && s.id === p.comparison ? { ...s, progress: p } : s));
      }
    });
    return () => void off.then((o) => o());
  }, [transport]);

  const close = useCallback(async () => {
    const id = idRef.current;
    idRef.current = null;
    setState({ phase: "idle" });
    if (id !== null) await transport.compareClose(id);
  }, [transport]);

  const start = useCallback(
    async (previous: string, current: string, settings: CompareSettings) => {
      if (idRef.current !== null) await close();
      settingsRef.current = settings;
      try {
        const id = await transport.compareStart(previous, current, settings);
        idRef.current = id;
        setState({ phase: "running", id, previous, current, progress: null });
      } catch (e) {
        setState({ phase: "failed", error: String(e) });
      }
    },
    [transport, close],
  );

  const cancel = useCallback(() => {
    if (idRef.current !== null) void transport.compareCancel(idRef.current);
  }, [transport]);

  /** New thresholds or tracked elements; counts update without a rescan. */
  const setElements = useCallback(
    async (elements: ElementSetting[]) => {
      const id = idRef.current;
      if (id === null) return;
      const deltas = await transport.compareSetElements(id, elements);
      setState((s) =>
        s.phase === "ready" && s.id === id ? { ...s, summary: { ...s.summary, elements: deltas }, settings: { ...s.settings, elements } } : s,
      );
    },
    [transport],
  );

  return { state, start, cancel, close, setElements };
}

export type Comparison = ReturnType<typeof useComparison>;

/** File name without folders, for headings. */
export const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

export const phaseLabel: Record<CompareProgress["phase"], string> = {
  open: "Opening both crawls",
  previous: "Reading the previous crawl",
  current: "Reading the current crawl",
  match: "Matching URLs",
  counts: "Counting changes",
  done: "Done",
  failed: "Failed",
  cancelled: "Cancelled",
};
