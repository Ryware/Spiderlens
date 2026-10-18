import { useCallback, useEffect, useRef, useState } from "react";
import type { Report } from "../generated/Report";
import type { Transport } from "../transport";

/** While a crawl runs the report is rebuilt this often (FR-006). */
export const LIVE_REFRESH_MS = 5000;

/**
 * The open crawl file's report. Built by the engine from stored data; while
 * the crawl runs it is refreshed and marked provisional. With an open
 * comparison of this file, the report carries its section (spec 012 FR-033).
 */
export function useReport(transport: Transport, path: string | null, running: boolean, comparison: number | null = null) {
  const [report, setReport] = useState<Report | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const seq = useRef(0);

  const load = useCallback(async () => {
    if (!path) return;
    const n = ++seq.current;
    setLoading(true);
    try {
      const r = await transport.report(path, running, null, comparison);
      if (n === seq.current) {
        setReport(r);
        setError(null);
      }
    } catch (e) {
      if (n === seq.current) setError(String(e));
    } finally {
      if (n === seq.current) setLoading(false);
    }
  }, [transport, path, running, comparison]);

  useEffect(() => {
    setReport(null);
    setError(null);
  }, [path]);

  useEffect(() => {
    void load();
    if (!running) return;
    const t = setInterval(() => void load(), LIVE_REFRESH_MS);
    return () => clearInterval(t);
  }, [load, running]);

  return { report, error, loading, reload: load };
}
