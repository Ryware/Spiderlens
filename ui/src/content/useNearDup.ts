import { useCallback, useEffect, useState } from "react";
import type { ContentSettings } from "../generated/ContentSettings";
import type { NearDupStatus } from "../generated/NearDupStatus";
import type { AnalysisFinished, AnalysisProgress, Transport } from "../transport";

/**
 * The open crawl's near duplicate analysis (spec 020): its status, the
 * running pass's progress, and a generation that changes whenever new
 * results land, so views over them reopen.
 */
export function useNearDup(transport: Transport, path: string | null, live: boolean) {
  const [status, setStatus] = useState<NearDupStatus | null>(null);
  const [progress, setProgress] = useState<AnalysisProgress | null>(null);
  const [finished, setFinished] = useState<AnalysisFinished | null>(null);
  const [generation, setGeneration] = useState(0);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(() => {
    if (!path) return;
    transport.nearDupStatus(path).then(
      (s) => {
        setStatus(s);
        setError(null);
      },
      // A new crawl's file can lag its start by a moment; the next event retries.
      () => {},
    );
  }, [transport, path]);

  useEffect(() => {
    setStatus(null);
    setProgress(null);
    setFinished(null);
    setError(null);
  }, [path]);

  // Again when a crawl stops: the analysis starts then.
  useEffect(reload, [reload, live]);

  useEffect(() => {
    if (!path) return;
    const offs = [
      transport.onAnalysisProgress((p) => {
        if (p.path !== path) return;
        setProgress(p);
        setStatus((s) => (s && s.state !== "running" ? { ...s, state: "running", reason: null } : s));
      }),
      transport.onAnalysisFinished((f) => {
        if (f.path !== path) return;
        setProgress(null);
        setFinished(f);
        setGeneration((g) => g + 1);
        reload();
      }),
    ];
    return () => offs.forEach((o) => o.then((off) => off()));
  }, [transport, path, reload]);

  const run = useCallback(
    async (settings: ContentSettings | null) => {
      if (!path) return;
      setFinished(null);
      try {
        await transport.runNearDup(path, settings);
        setStatus((s) => (s ? { ...s, state: "running", reason: null } : s));
        setError(null);
      } catch (e) {
        setError(String(e));
      }
    },
    [transport, path],
  );

  const cancel = useCallback(() => {
    if (path) void transport.cancelNearDup(path);
  }, [transport, path]);

  return { status, progress, finished, generation, error, run, cancel, reload };
}

export type NearDup = ReturnType<typeof useNearDup>;
