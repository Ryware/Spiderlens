import type { Tone } from "../components/ui";
import type { CrawlConfig } from "../generated/CrawlConfig";
import type { Registered } from "../generated/Registered";
import type { RunStatus } from "../generated/RunStatus";
import type { Schedule } from "../generated/Schedule";
import type { Trigger } from "../generated/Trigger";

/** How each run status reads (spec 021 FR-018). */
export const runStatusInfo: Record<RunStatus, { label: string; tone: Tone }> = {
  running: { label: "Running", tone: "info" },
  finished: { label: "Finished", tone: "ok" },
  stopped_at_limit: { label: "Stopped at limit", tone: "ok" },
  stopped_by_user: { label: "Stopped by user", tone: "neutral" },
  failed: { label: "Failed", tone: "bad" },
  paused_disk: { label: "Paused: disk", tone: "warn" },
  interrupted: { label: "Interrupted", tone: "warn" },
  skipped: { label: "Skipped", tone: "neutral" },
  missed: { label: "Missed", tone: "warn" },
};

export const triggerLabel: Record<Trigger, string> = {
  scheduled: "Scheduled",
  caught_up: "Caught up",
  run_now: "Run now",
};

/** Off, or why the system scheduler has no registration; null when all is well. */
export function registrationNote(r: Registered): { label: string; tone: Tone; detail: string | null } | null {
  if (r.state === "suspended") return { label: "Off", tone: "neutral", detail: null };
  if (r.state === "not_registered") return { label: "Not registered", tone: "bad", detail: r.reason };
  return null;
}

const when = new Intl.DateTimeFormat(undefined, { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
const whenYear = new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit" });

/** A run time in the computer's local time, as the schedule is defined. */
export function formatRunTime(iso: string | null, now = new Date()): string {
  if (!iso) return "–";
  const d = new Date(iso);
  return d.getFullYear() === now.getFullYear() ? when.format(d) : whenYear.format(d);
}

export function formatDuration(start: string | null, end: string | null): string {
  if (!start || !end) return "–";
  const s = Math.max(0, Math.round((Date.parse(end) - Date.parse(start)) / 1000));
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min ${s % 60} s`;
  return `${Math.floor(m / 60)} h ${m % 60} min`;
}

export const DEFAULT_KEEP_LAST = 10;

/** What a schedule does with old run files: "Keeps the last 10 runs". */
export function keepLastText(keepLast: number | null): string {
  if (keepLast === null) return "Keeps all runs";
  return keepLast === 1 ? "Keeps the last run" : `Keeps the last ${keepLast} runs`;
}

export const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

/** A new schedule's name from what it crawls: "example.com weekly". */
export function suggestName(startUrl: string | null, listPath: string | null): string {
  if (startUrl) {
    try {
      return `${new URL(startUrl.includes("://") ? startUrl : `https://${startUrl}`).host} weekly`;
    } catch {
      // Fall through to the generic name.
    }
  }
  if (listPath) return `${fileName(listPath).replace(/\.[^.]+$/, "")} weekly`;
  return "Weekly crawl";
}

/** A new schedule from a configuration and its input (FR-001, FR-002). */
export function newDraft({
  config,
  startUrl,
  listPath = null,
  profile = null,
  outputDir = "",
}: {
  config: CrawlConfig;
  startUrl: string | null;
  listPath?: string | null;
  profile?: string | null;
  outputDir?: string;
}): Schedule {
  const now = new Date().toISOString();
  return {
    id: "",
    name: suggestName(config.mode === "spider" ? startUrl : null, config.mode === "list" ? listPath : null),
    enabled: true,
    config: structuredClone(config),
    source_profile: profile,
    source_hash: null,
    input: config.mode === "list" ? { list: { path: listPath ?? "" } } : { spider: { start_url: startUrl ?? "" } },
    // Weekly, Monday 06:00: a quiet time for most sites.
    frequency: { kind: "weekly", days: [1], time: { hour: 6, minute: 0 } },
    output_dir: outputDir,
    catch_up: true,
    notify: "problems",
    after: [],
    keep_last: DEFAULT_KEEP_LAST,
    created_at: now,
    updated_at: now,
  };
}
