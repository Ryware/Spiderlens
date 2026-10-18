// In-memory schedules (spec 021) for UI tests. Frequencies, validation and
// next-run times here are small stand-ins; the real rules are tested in Rust
// (crates/schedule).

import type { Frequency } from "../generated/Frequency";
import type { Issue } from "../generated/Issue";
import type { LiveRun } from "../generated/LiveRun";
import type { Preview } from "../generated/Preview";
import type { Registered } from "../generated/Registered";
import type { RepairReport } from "../generated/RepairReport";
import type { Run } from "../generated/Run";
import type { Schedule } from "../generated/Schedule";
import type { ScheduleDetail } from "../generated/ScheduleDetail";
import type { ScheduleRow } from "../generated/ScheduleRow";
import type { ScheduleSaved, Transport } from "../transport";
import { defaultConfig } from "./fakeTransport";

/** "Now" for every fake preview: Wednesday 14 October 2026, 09:30 local. */
export const FAKE_NOW = new Date(2026, 9, 14, 9, 30);

const DAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const pad = (n: number) => String(n).padStart(2, "0");
const hhmm = (t: { hour: number; minute: number }) => `${pad(t.hour)}:${pad(t.minute)}`;

export function describeFrequency(f: Frequency): string {
  switch (f.kind) {
    case "hours":
      return f.every === 1 ? `Every hour at :${pad(f.minute)}` : `Every ${f.every} hours at :${pad(f.minute)}`;
    case "daily":
      return `Every day at ${hhmm(f.time)}`;
    case "weekly":
      return `Every ${f.days.map((d) => DAYS[d % 7]).join(", ")} at ${hhmm(f.time)}`;
    case "monthly":
      return `Monthly on ${f.day === "last" ? "the last day" : `day ${f.day.day}`} at ${hhmm(f.time)}`;
    case "advanced":
      return `Cron ${f.cron}`;
  }
}

function frequencyError(f: Frequency): string | null {
  if (f.kind === "hours" && (f.every < 1 || f.every > 24)) return "Every 1 to 24 hours.";
  if (f.kind === "weekly" && f.days.length === 0) return "Choose at least one day.";
  if (f.kind === "monthly" && f.day !== "last" && (f.day.day < 1 || f.day.day > 28)) return "Choose a day from 1 to 28, or the last day.";
  if (f.kind === "advanced") {
    const parts = f.cron.trim().split(/\s+/);
    if (parts.length !== 5) return "A cron expression has five fields: minute, hour, day, month, weekday.";
    if (!/^\d{1,2}$/.test(parts[0])) return "Runs more often than once an hour. Use a single minute, such as 0.";
  }
  return null;
}

function nextRuns(f: Frequency, from = FAKE_NOW, count = 5): string[] {
  const out: Date[] = [];
  const d = new Date(from);
  d.setSeconds(0, 0);
  // Minute by minute is fine for a fake: at most a few months ahead.
  for (let i = 0; i < 60 * 24 * 120 && out.length < count; i++) {
    d.setMinutes(d.getMinutes() + 1);
    if (matches(f, d)) out.push(new Date(d));
  }
  return out.map((x) => x.toISOString());
}

function matches(f: Frequency, d: Date): boolean {
  const at = (t: { hour: number; minute: number }) => d.getHours() === t.hour && d.getMinutes() === t.minute;
  switch (f.kind) {
    case "hours":
      return d.getMinutes() === f.minute && d.getHours() % f.every === 0;
    case "daily":
      return at(f.time);
    case "weekly":
      return f.days.some((x) => x % 7 === d.getDay()) && at(f.time);
    case "monthly": {
      const last = new Date(d.getFullYear(), d.getMonth() + 1, 0).getDate();
      return (f.day === "last" ? d.getDate() === last : d.getDate() === f.day.day) && at(f.time);
    }
    case "advanced":
      return d.getMinutes() === Number(f.cron.trim().split(/\s+/)[0]);
  }
}

export function fakePreview(f: Frequency): Preview {
  const error = frequencyError(f);
  return { describe: error ? "" : describeFrequency(f), next_runs: error ? [] : nextRuns(f), error };
}

export function fakeSchedule(patch: Partial<Schedule> = {}): Schedule {
  return {
    id: "",
    name: "Weekly site check",
    enabled: true,
    config: defaultConfig(),
    source_profile: null,
    source_hash: null,
    input: { spider: { start_url: "https://example.com/" } },
    frequency: { kind: "weekly", days: [1], time: { hour: 6, minute: 0 } },
    output_dir: "/Users/me/Documents/SpiderLens schedules",
    catch_up: true,
    notify: "problems",
    after: [],
    keep_last: 10,
    created_at: FAKE_NOW.toISOString(),
    updated_at: FAKE_NOW.toISOString(),
    ...patch,
  };
}

export function fakeRun(run: number, patch: Partial<Run> = {}): Run {
  return {
    run,
    slot: new Date(2026, 9, 12, 6, 0).toISOString(),
    trigger: "scheduled",
    status: "finished",
    reason: null,
    started_at: new Date(2026, 9, 12, 6, 0, 2).toISOString(),
    ended_at: new Date(2026, 9, 12, 6, 4, 40).toISOString(),
    counts: { discovered: 1240, crawled: 1200, not_crawled: 40, failed: 3 },
    file: `/Users/me/Documents/SpiderLens schedules/Weekly site check 2026-10-12 0600.spiderlens`,
    steps: [],
    ...patch,
  };
}

function validate(s: Schedule): Issue[] {
  const issues: Issue[] = [];
  const err = (path: string, code: string, message: string) => issues.push({ path, severity: "error", code, message });
  if (!s.name.trim()) err("name", "name_empty", "Give the schedule a name.");
  const f = frequencyError(s.frequency);
  if (f) err("frequency", "frequency_invalid", f);
  if (!s.output_dir.trim() || !(s.output_dir.startsWith("/") || /^[A-Za-z]:\\/.test(s.output_dir))) {
    err("output_dir", "output_relative", "Choose a full folder path for the crawl files.");
  }
  if (s.keep_last !== null && (!Number.isInteger(s.keep_last) || s.keep_last < 1 || s.keep_last > 999)) {
    err("keep_last", "keep_last_range", "Keep between 1 and 999 runs, or keep all of them.");
  }
  const secret = s.config.identity.headers.find((h) => "secret" in h.value);
  if (secret) {
    err("config.identity.headers", "secret_header", `The header "${secret.name}" is marked secret. Secrets are never stored, so this crawl cannot run on a schedule.`);
  }
  if ("spider" in s.input && !s.input.spider.start_url.trim()) err("input", "start_url_empty", "Enter the start URL.");
  if ("list" in s.input && !s.input.list.path.trim()) err("input", "list_missing", "Choose the list file.");
  return issues;
}

export interface FakeSchedules {
  stored: Map<string, Schedule>;
  history: Map<string, Run[]>;
  running: Map<string, LiveRun>;
  /** The OS refuses registrations with this reason when set. */
  refuse: string | null;
  /** Source profiles whose content changed after the schedule copied them. */
  changedProfiles: Set<string>;
  repair: RepairReport;
  repairs: number;
  ranNow: string[];
  stopped: string[];
  deleted: string[];
  seed(s: Partial<Schedule>, runs?: Run[]): Schedule;
}

type ScheduleApi = Pick<
  Transport,
  | "schedulesList"
  | "scheduleGet"
  | "schedulePreview"
  | "scheduleValidate"
  | "scheduleSave"
  | "scheduleSetEnabled"
  | "scheduleDuplicate"
  | "scheduleDelete"
  | "scheduleRunNow"
  | "scheduleStop"
  | "scheduleHistory"
  | "scheduleUpdateFromProfile"
  | "schedulesRepair"
  | "schedulesRemoveAll"
  | "scheduleDefaultOutput"
  | "pickFolder"
>;

export function fakeSchedulesTransport(profileConfig: (name: string) => Schedule["config"] | null) {
  let n = 0;
  const newId = (name: string) => `${name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "schedule"}-${++n}`;
  const registration = (s: Schedule): Registered =>
    !s.enabled ? { state: "suspended" } : schedules.refuse ? { state: "not_registered", reason: schedules.refuse } : { state: "registered", program: "/Applications/SpiderLens.app" };
  const get = (id: string) => {
    const s = schedules.stored.get(id);
    if (!s) throw `No schedule "${id}".`;
    return s;
  };
  const row = (s: Schedule): ScheduleRow => {
    const runs = schedules.history.get(s.id) ?? [];
    return {
      schedule: structuredClone(s),
      describe: describeFrequency(s.frequency),
      next_run: s.enabled ? (nextRuns(s.frequency, FAKE_NOW, 1)[0] ?? null) : null,
      last: runs.length ? structuredClone(runs[runs.length - 1]) : null,
      registration: registration(s),
      running: schedules.running.get(s.id) ?? null,
    };
  };
  const detail = (s: Schedule): ScheduleDetail => ({
    row: row(s),
    next_runs: s.enabled ? nextRuns(s.frequency) : [],
    profile_changed: !!s.source_profile && schedules.changedProfiles.has(s.source_profile),
  });
  const saved = (s: Schedule, warnings: Issue[] = []): ScheduleSaved => ({
    detail: detail(s),
    warnings,
    refused: s.enabled ? schedules.refuse : null,
  });
  const put = (s: Schedule) => {
    const issues = validate(s);
    const errors = issues.filter((i) => i.severity === "error");
    if (errors.length) throw errors.map((i) => i.message).join(" ");
    const now = new Date().toISOString();
    const out = { ...structuredClone(s), name: s.name.trim(), id: s.id || newId(s.name), updated_at: now };
    schedules.stored.set(out.id, out);
    return out;
  };

  const schedules: FakeSchedules = {
    stored: new Map(),
    history: new Map(),
    running: new Map(),
    refuse: null,
    changedProfiles: new Set(),
    repair: { repaired: [], failed: [], orphans: [] },
    repairs: 0,
    ranNow: [],
    stopped: [],
    deleted: [],
    seed: (patch, runs = []) => {
      const s = fakeSchedule(patch);
      if (!s.id) s.id = newId(s.name);
      schedules.stored.set(s.id, s);
      schedules.history.set(s.id, runs);
      return s;
    },
  };

  const api: ScheduleApi = {
    schedulesList: async () => [...schedules.stored.values()].sort((a, b) => a.name.localeCompare(b.name)).map(row),
    scheduleGet: async (id) => detail(get(id)),
    schedulePreview: async (f) => fakePreview(f),
    scheduleValidate: async (draft) => validate(draft),
    scheduleSave: async (draft) => saved(put(draft)),
    scheduleSetEnabled: async (id, enabled) => saved(put({ ...get(id), enabled })),
    scheduleDuplicate: async (id, name) => put({ ...get(id), id: "", name, enabled: false }),
    scheduleDelete: async (id) => {
      get(id);
      schedules.deleted.push(id);
      schedules.stored.delete(id);
    },
    scheduleRunNow: async (id) => {
      get(id);
      schedules.ranNow.push(id);
    },
    scheduleStop: async (id) => void schedules.stopped.push(id),
    scheduleHistory: async (id, limit) => structuredClone(schedules.history.get(id) ?? []).reverse().slice(0, limit),
    scheduleUpdateFromProfile: async (id) => {
      const s = get(id);
      if (!s.source_profile) throw "This schedule was not made from a profile.";
      const config = profileConfig(s.source_profile);
      if (!config) throw `The profile "${s.source_profile}" no longer exists.`;
      schedules.changedProfiles.delete(s.source_profile);
      return saved(put({ ...s, config: structuredClone(config) }));
    },
    schedulesRepair: async () => {
      schedules.repairs++;
      return structuredClone(schedules.repair);
    },
    schedulesRemoveAll: async () => {
      const count = schedules.stored.size;
      schedules.stored.clear();
      return count;
    },
    scheduleDefaultOutput: async () => "/Users/me/Documents/SpiderLens schedules",
    pickFolder: async () => "/Users/me/Crawls",
  };
  return { schedules, api };
}
