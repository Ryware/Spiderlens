// Shared bits of the URL Inspection and Analytics run panels (spec 024
// slices 2 and 4): how a run's state reads, and the list of a provider's runs
// in the open crawl.

import type { ReactNode } from "react";
import { Badge } from "../components/ui";
import type { RunProgress } from "../generated/RunProgress";
import type { RunState } from "../generated/RunState";

export const STATE_LABEL: Record<RunState, string> = {
  queued: "Queued",
  running: "Running",
  finished: "Finished",
  paused_user: "Paused",
  paused_quota: "Paused: allowance used",
  cancelled: "Cancelled",
  failed: "Failed",
};

export const STATE_TONE: Record<RunState, "ok" | "info" | "warn" | "bad" | "neutral"> = {
  queued: "neutral",
  running: "info",
  finished: "ok",
  paused_user: "warn",
  paused_quota: "warn",
  cancelled: "neutral",
  failed: "bad",
};

export const resumable = (s: RunState) => s === "paused_user" || s === "paused_quota" || s === "running" || s === "queued";

export const num = (v: number | null) => (v ?? 0).toLocaleString();

export function errorMessage(e: unknown) {
  return e instanceof Error ? e.message : String(e);
}

/** A provider's runs in the open crawl, newest first, with when a paused run can continue. */
export function RunList({ label, runs, summary }: { label: string; runs: RunProgress[]; summary: (r: RunProgress) => ReactNode }) {
  if (runs.length === 0) return null;
  return (
    <ul aria-label={label} className="m-0 flex list-none flex-col gap-1 p-0">
      {runs.map((r) => (
        <li key={r.id} data-run={r.id} className="flex flex-wrap items-center gap-2 text-sm text-fg-2">
          <Badge tone={STATE_TONE[r.state]}>{STATE_LABEL[r.state]}</Badge>
          <span className="text-fg">{r.property}</span>
          <span>{summary(r)}</span>
          {r.state === "paused_quota" && r.resume_at && <span className="text-warn">Resume after {new Date(r.resume_at).toLocaleString()}</span>}
          {r.message && r.state !== "finished" && <span className="text-fg-3">{r.message}</span>}
        </li>
      ))}
    </ul>
  );
}
