import { useCallback, useEffect, useRef, useState } from "react";
import type { AeoRule } from "../generated/AeoRule";
import type { AiProgress } from "../generated/AiProgress";
import type { AiSummary } from "../generated/AiSummary";
import type { Transport } from "../transport";

/** The checks' names and fixes, read once per app run. */
let rulesCache: Promise<Map<string, AeoRule>> | null = null;

export function useAeoRules(transport: Transport) {
  const [rules, setRules] = useState<Map<string, AeoRule>>(new Map());
  useEffect(() => {
    let off = false;
    rulesCache ??= transport.aeoRules().then((rs) => new Map(rs.map((r) => [r.rule.id, r])));
    rulesCache.then(
      (m) => !off && setRules(m),
      () => (rulesCache = null),
    );
    return () => {
      off = true;
    };
  }, [transport]);
  return rules;
}

/** Tests start each case without the previous case's rules. */
export const resetAeoRules = () => (rulesCache = null);

/**
 * The open file's AI checks (spec 015): stored results, a run in progress
 * (started here or by the app when a crawl completes) and its outcome.
 */
export function useAeo(transport: Transport, path: string) {
  const [summary, setSummary] = useState<AiSummary | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [running, setRunning] = useState<{ runId: number | null; progress: AiProgress | null } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [cancelled, setCancelled] = useState(false);
  const runRef = useRef(running);
  runRef.current = running;

  useEffect(() => {
    let off = false;
    setSummary(null);
    setLoaded(false);
    setRunning(null);
    setError(null);
    transport.aeoSummary(path).then(
      (s) => {
        if (off) return;
        setSummary(s);
        setLoaded(true);
      },
      (e) => {
        if (off) return;
        setError(String(e));
        setLoaded(true);
      },
    );
    return () => {
      off = true;
    };
  }, [transport, path]);

  useEffect(() => {
    const offs = [
      transport.onAeoProgress((p) => {
        if (p.path !== path) return;
        setRunning({ runId: p.run_id, progress: p });
      }),
      transport.onAeoFinished((f) => {
        if (f.path !== path) return;
        setRunning(null);
        if (f.summary) setSummary(f.summary);
        setError(f.error);
        setCancelled(!f.summary && !f.error);
      }),
    ];
    return () => offs.forEach((o) => void o.then((f) => f()));
  }, [transport, path]);

  const run = useCallback(async () => {
    setError(null);
    setCancelled(false);
    setRunning({ runId: null, progress: null });
    try {
      const { run_id } = await transport.aeoRun(path);
      setRunning((r) => (r ? { ...r, runId: run_id } : r));
    } catch (e) {
      setRunning(null);
      setError(String(e));
    }
  }, [transport, path]);

  const cancel = useCallback(() => {
    const id = runRef.current?.runId;
    if (id != null) void transport.aeoCancel(id);
  }, [transport]);

  return { summary, loaded, running, error, cancelled, run, cancel };
}
