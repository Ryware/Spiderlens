import { useCallback, useEffect, useRef, useState } from "react";
import type { Segment } from "../generated/Segment";
import type { ViewQuery } from "../generated/ViewQuery";
import type { ExportProgress, Range, Transport } from "../transport";

/** One CSV export at a time, with progress and cancel (FR-028..FR-030). */
export function useExport(transport: Transport) {
  const [job, setJob] = useState<number | null>(null);
  const [progress, setProgress] = useState<ExportProgress | null>(null);
  const [result, setResult] = useState<ExportProgress | null>(null);
  const jobRef = useRef<number | null>(null);

  useEffect(() => {
    const off = transport.onExportProgress((p) => {
      if (p.job !== jobRef.current) return;
      if (p.state === "running") setProgress(p);
      else {
        jobRef.current = null;
        setJob(null);
        setProgress(null);
        setResult(p);
      }
    });
    return () => void off.then((o) => o());
  }, [transport]);

  useEffect(() => {
    if (!result || result.state === "failed") return;
    const t = setTimeout(() => setResult(null), 6000);
    return () => clearTimeout(t);
  }, [result]);

  /** Runs any export command that answers with a job id (spec 012 uses it too). */
  const run = useCallback(
    async (begin: () => Promise<number>, dest: string) => {
      setResult(null);
      try {
        const id = await begin();
        jobRef.current = id;
        setJob(id);
        setProgress({ job: id, rows_written: 0, total: 0, state: "running", error: null, path: dest });
      } catch (e) {
        setResult({ job: 0, rows_written: 0, total: 0, state: "failed", error: String(e), path: dest });
      }
    },
    [],
  );

  const start = useCallback(
    (path: string, query: ViewQuery, columns: string[], ranges: Range[] | null, dest: string, allValues?: string | null, segments?: Segment[]) =>
      run(() => transport.exportStart(path, query, columns, ranges, dest, allValues, segments), dest),
    [transport, run],
  );

  const cancel = useCallback(() => {
    if (jobRef.current !== null) void transport.exportCancel(jobRef.current);
  }, [transport]);

  return { job, progress, result, start, run, cancel };
}
