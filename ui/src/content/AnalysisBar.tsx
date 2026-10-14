// The analysis state above the Content tab (FR-012, FR-013, FR-022).

import { RefreshCw } from "lucide-react";
import type { ContentSettings } from "../generated/ContentSettings";
import { Bar, Button, Notice } from "../components/ui";
import type { NearDup } from "./useNearDup";

const n = (x: number) => x.toLocaleString();

export function AnalysisBar({ nd, live, settings }: { nd: NearDup; live: boolean; settings: ContentSettings | null }) {
  const s = nd.status;
  if (!s) return null;
  const running = s.state === "running";
  const excluded = s.too_short + s.not_indexable + s.no_signature;
  return (
    <div role="status" aria-label="Near duplicate analysis" className="flex min-h-9 items-center gap-3 border-b border-line bg-chrome px-3 py-1.5 text-sm">
      {running ? (
        <>
          <span className="text-fg-2">{nd.progress?.step ?? "Analysing content"}…</span>
          <Bar value={nd.progress && nd.progress.total ? nd.progress.done / nd.progress.total : 0} tone="accent" label="Analysis progress" className="w-40" />
          <Button size="sm" variant="quiet" className="ml-auto" onClick={nd.cancel}>
            Cancel
          </Button>
        </>
      ) : s.state === "done" || s.state === "stale" ? (
        <>
          <span className="text-fg-2 tabular" data-testid="analysis-summary">
            {n(s.near_duplicates)} of {n(s.compared)} pages are near duplicates at {s.threshold ?? 90}%, in {n(s.clusters)} {s.clusters === 1 ? "cluster" : "clusters"}
            {excluded > 0 && <span className="text-fg-3"> · {n(excluded)} not compared</span>}
          </span>
          {s.state === "stale" && <Notice kind="warn">Out of date: {s.reason}.</Notice>}
          <Button size="sm" variant={s.state === "stale" ? "secondary" : "quiet"} className="ml-auto" disabled={live} onClick={() => void nd.run(settings)}>
            <RefreshCw aria-hidden className="size-3.5" />
            Analyse again
          </Button>
        </>
      ) : s.state === "unavailable" ? (
        <span className="text-fg-3">{s.reason}</span>
      ) : (
        <>
          <span className="text-fg-3">{live ? "Near duplicates are found when the crawl finishes or is paused." : (s.reason ?? "Not analysed yet.")}</span>
          {!live && (
            <Button size="sm" variant="secondary" className="ml-auto" onClick={() => void nd.run(settings)}>
              Find near duplicates
            </Button>
          )}
        </>
      )}
      {nd.finished?.error && <Notice kind="bad">The analysis did not finish: {nd.finished.error}</Notice>}
      {nd.error && <Notice kind="bad">{nd.error}</Notice>}
    </div>
  );
}
