import { useCallback, useEffect, useRef, useState } from "react";
import type { CustomRules } from "../generated/CustomRules";
import type { CustomRunDone, CustomRunProgress, Transport } from "../transport";

/**
 * Running custom rules on a finished crawl's stored HTML (spec 013 US3):
 * start, progress, cancel and the outcome. Nothing is fetched.
 */
export function useCustomRun(transport: Transport, path: string | null) {
  const [run, setRun] = useState<number | null>(null);
  const [progress, setProgress] = useState<CustomRunProgress | null>(null);
  const [result, setResult] = useState<CustomRunDone | null>(null);
  const runRef = useRef<number | null>(null);
  runRef.current = run;

  useEffect(() => {
    const offs = [
      transport.onCustomRunProgress((p) => {
        if (p.run === runRef.current) setProgress(p);
      }),
      transport.onCustomRunDone((d) => {
        if (d.run !== runRef.current) return;
        setRun(null);
        setProgress(null);
        setResult(d);
      }),
    ];
    return () => offs.forEach((o) => o.then((off) => off()));
  }, [transport]);

  useEffect(() => {
    setResult(null);
  }, [path]);

  /** `rules` null runs the rules stored with the crawl again. */
  const start = useCallback(
    async (rules: CustomRules | null) => {
      if (!path || runRef.current !== null) return;
      setResult(null);
      setProgress({ run: 0, done: 0, total: 0, per_sec: 0 });
      try {
        const id = await transport.customRun(path, rules);
        runRef.current = id;
        setRun(id);
      } catch (e) {
        setProgress(null);
        setResult({ run: 0, path, state: "failed", pages: 0, error: String(e) });
      }
    },
    [transport, path],
  );

  const cancel = useCallback(() => {
    if (runRef.current !== null) void transport.customCancel(runRef.current);
  }, [transport]);

  return { running: progress !== null, progress, result, start, cancel };
}

export type CustomRun = ReturnType<typeof useCustomRun>;

/** "Ran on 1,234 pages", "Stopped after 500 pages", or the reason it could not run. */
export function runOutcomeText(d: CustomRunDone): string {
  const pages = `${d.pages.toLocaleString()} ${d.pages === 1 ? "page" : "pages"}`;
  if (d.state === "complete") return `Rules ran on ${pages} from stored HTML. No requests were sent.`;
  if (d.state === "partial") return `Stopped after ${pages}. Pages after that keep their earlier values.`;
  return d.error ?? "The rules could not run.";
}

/** The custom extraction and search tabs (spec 013); their ids never change. */
export const isCustomTab = (id: string) => id === "custom_extraction" || id === "custom_search";
