import { useCallback, useEffect, useRef, useState } from "react";
/** One CSV export at a time, with progress and cancel (FR-028..FR-030). */
export function useExport(transport) {
    const [job, setJob] = useState(null);
    const [progress, setProgress] = useState(null);
    const [result, setResult] = useState(null);
    const jobRef = useRef(null);
    useEffect(() => {
        const off = transport.onExportProgress((p) => {
            if (p.job !== jobRef.current)
                return;
            if (p.state === "running")
                setProgress(p);
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
        if (!result || result.state === "failed")
            return;
        const t = setTimeout(() => setResult(null), 6000);
        return () => clearTimeout(t);
    }, [result]);
    const start = useCallback(async (path, query, columns, ranges, dest) => {
        setResult(null);
        try {
            const id = await transport.exportStart(path, query, columns, ranges, dest);
            jobRef.current = id;
            setJob(id);
            setProgress({ job: id, rows_written: 0, total: 0, state: "running", error: null, path: dest });
        }
        catch (e) {
            setResult({ job: 0, rows_written: 0, total: 0, state: "failed", error: String(e), path: dest });
        }
    }, [transport]);
    const cancel = useCallback(() => {
        if (jobRef.current !== null)
            void transport.exportCancel(jobRef.current);
    }, [transport]);
    return { job, progress, result, start, cancel };
}
