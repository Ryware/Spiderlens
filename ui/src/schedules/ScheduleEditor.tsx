import { useEffect, useRef, useState } from "react";
import type { AfterStep } from "../generated/AfterStep";
import type { Frequency } from "../generated/Frequency";
import type { Issue } from "../generated/Issue";
import type { NotifyLevel } from "../generated/NotifyLevel";
import type { Preview } from "../generated/Preview";
import type { Run } from "../generated/Run";
import type { Schedule } from "../generated/Schedule";
import type { TabDef } from "../generated/TabDef";
import type { TimeOfDay } from "../generated/TimeOfDay";
import type { ScheduleSaved, Transport } from "../transport";
import { Button, cn, Input, Notice, NumberInput, Segmented, Select, Sheet, SheetClose, Switch } from "../components/ui";
import { Group, IssueList, issuesAt, Row } from "../settings/fields";
import { formatRunTime, DEFAULT_KEEP_LAST } from "./format";

/** How long to wait after a change before asking for the preview and checks. */
export const EDITOR_DEBOUNCE_MS = 150;

type Kind = Frequency["kind"];
const kinds: { value: Kind; label: string }[] = [
  { value: "hours", label: "Hourly" },
  { value: "daily", label: "Daily" },
  { value: "weekly", label: "Weekly" },
  { value: "monthly", label: "Monthly" },
  { value: "advanced", label: "Advanced" },
];
// Monday first, as most calendars show it; the engine numbers Sunday 0.
const weekdays = [
  { d: 1, short: "Mon", name: "Monday" },
  { d: 2, short: "Tue", name: "Tuesday" },
  { d: 3, short: "Wed", name: "Wednesday" },
  { d: 4, short: "Thu", name: "Thursday" },
  { d: 5, short: "Fri", name: "Friday" },
  { d: 6, short: "Sat", name: "Saturday" },
  { d: 0, short: "Sun", name: "Sunday" },
];
const notifyOptions: { value: NotifyLevel; label: string }[] = [
  { value: "always", label: "Always" },
  { value: "problems", label: "Problems only" },
  { value: "never", label: "Never" },
];

/** Issues shown at their field; the rest (the crawl settings) show at the bottom. */
const shownAtField = ["name", "input", "frequency", "output_dir", "keep_last"];

const timeOf = (f: Frequency): TimeOfDay => ("time" in f ? f.time : { hour: 6, minute: 0 });
const pad = (n: number) => String(n).padStart(2, "0");

/** Switching preset keeps the time of day the user already chose. */
function switchKind(f: Frequency, kind: Kind): Frequency {
  const time = timeOf(f);
  switch (kind) {
    case "hours":
      return { kind, every: 6, minute: f.kind === "hours" ? f.minute : time.minute };
    case "daily":
      return { kind, time };
    case "weekly":
      return { kind, days: f.kind === "weekly" ? f.days : [1], time };
    case "monthly":
      return { kind, day: { day: 1 }, time };
    case "advanced":
      return { kind, cron: f.kind === "advanced" ? f.cron : `${time.minute} ${time.hour} * * 1` };
  }
}

function TimeField({ id, value, onChange }: { id: string; value: TimeOfDay; onChange: (t: TimeOfDay) => void }) {
  return (
    <Input
      id={id}
      type="time"
      className="w-28 tabular"
      value={`${pad(value.hour)}:${pad(value.minute)}`}
      onChange={(e) => {
        const [h, m] = e.target.value.split(":").map(Number);
        if (Number.isFinite(h) && Number.isFinite(m)) onChange({ hour: h, minute: m });
      }}
    />
  );
}

function FrequencyFields({ value, onChange, issues }: { value: Frequency; onChange: (f: Frequency) => void; issues: Issue[] }) {
  const f = value;
  return (
    <>
      <Row label="Repeat" issues={issues}>
        {() => <Segmented label="Repeat" value={f.kind} options={kinds} onChange={(k) => onChange(switchKind(f, k))} />}
      </Row>
      {f.kind === "hours" && (
        <Row label="Every" help="Hours count from midnight, so every 6 hours runs at 00, 06, 12 and 18.">
          {(id) => (
            <span className="flex items-center gap-2 text-sm text-fg-2">
              <NumberInput id={id} aria-label="Every how many hours" className="w-16" min={1} max={24} value={f.every} onChange={(v) => onChange({ ...f, every: v ?? 1 })} />
              hours, at minute
              <NumberInput aria-label="Minute past the hour" className="w-16" min={0} max={59} value={f.minute} onChange={(v) => onChange({ ...f, minute: v ?? 0 })} />
            </span>
          )}
        </Row>
      )}
      {f.kind === "weekly" && (
        <Row label="On">
          {() => (
            <div role="group" aria-label="Days of the week" className="flex gap-1">
              {weekdays.map((w) => {
                const on = f.days.includes(w.d);
                return (
                  <button
                    key={w.d}
                    type="button"
                    aria-pressed={on}
                    aria-label={w.name}
                    onClick={() => onChange({ ...f, days: on ? f.days.filter((d) => d !== w.d) : [...f.days, w.d].sort() })}
                    className={cn(
                      "h-control w-10 rounded-md border text-sm transition-colors duration-(--dur-fast)",
                      on ? "border-accent bg-accent text-accent-fg" : "border-line bg-surface text-fg-2 hover:bg-surface-2",
                    )}
                  >
                    {w.short}
                  </button>
                );
              })}
            </div>
          )}
        </Row>
      )}
      {f.kind === "monthly" && (
        <Row label="Day of the month" help="Days 29 to 31 do not exist in every month, so choose the last day instead.">
          {(id) => (
            <Select
              id={id}
              className="w-36"
              value={f.day === "last" ? "last" : String(f.day.day)}
              onChange={(e) => onChange({ ...f, day: e.target.value === "last" ? "last" : { day: Number(e.target.value) } })}
            >
              {Array.from({ length: 28 }, (_, i) => (
                <option key={i + 1} value={i + 1}>
                  {i + 1}
                </option>
              ))}
              <option value="last">Last day</option>
            </Select>
          )}
        </Row>
      )}
      {(f.kind === "daily" || f.kind === "weekly" || f.kind === "monthly") && (
        <Row label="At" help="Local time on this computer.">
          {(id) => <TimeField id={id} value={f.time} onChange={(time) => onChange({ ...f, time })} />}
        </Row>
      )}
      {f.kind === "advanced" && (
        <Row label="Cron expression" help="Five fields: minute, hour, day of month, month, day of week. At most once an hour.">
          {(id) => <Input id={id} mono className="w-56" value={f.cron} spellCheck={false} onChange={(e) => onChange({ ...f, cron: e.target.value })} />}
        </Row>
      )}
    </>
  );
}

/** The next five runs, as the engine computes them (FR-005). */
function NextRuns({ preview }: { preview: Preview | null }) {
  if (!preview) return null;
  if (preview.error) {
    return (
      <Notice kind="bad" role="alert" className="px-3.5 py-2.5">
        {preview.error}
      </Notice>
    );
  }
  return (
    <div className="flex flex-col gap-1 px-3.5 py-2.5">
      <div className="text-sm text-fg">{preview.describe}</div>
      <ol aria-label="Next runs" className="m-0 flex list-none flex-col gap-0.5 p-0 text-sm text-fg-2 tabular">
        {preview.next_runs.map((t) => (
          <li key={t}>{formatRunTime(t)}</li>
        ))}
      </ol>
    </div>
  );
}

function exportTabs(after: AfterStep[]): string[] {
  const e = after.find((a) => a.kind === "export");
  return e && e.kind === "export" ? e.tabs.map((t) => t.tab) : [];
}

function withExportTabs(after: AfterStep[], tabs: string[]): AfterStep[] {
  const rest = after.filter((a) => a.kind !== "export");
  return tabs.length ? [{ kind: "export", tabs: tabs.map((tab) => ({ tab, filter: "all" })) }, ...rest] : rest;
}

function hasCompare(after: AfterStep[]): boolean {
  return after.some((a) => a.kind === "compare");
}

function withCompare(after: AfterStep[], on: boolean): AfterStep[] {
  const rest = after.filter((a) => a.kind !== "compare");
  return on ? [...rest, { kind: "compare" }] : rest;
}

/**
 * Create or edit a schedule (spec 021 US1, US2): name, input, frequency with
 * the next five runs, output folder, missed runs, notifications and the
 * after-run steps. The engine validates; this only shows what comes back.
 */
export function ScheduleEditor({
  open,
  onOpenChange,
  transport,
  initial,
  onSaved,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  transport: Transport;
  /** The draft to start from: a new schedule, or a copy of an existing one. */
  initial: Schedule | null;
  onSaved: (s: ScheduleSaved) => void;
}) {
  const [draft, setDraft] = useState<Schedule | null>(initial);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [issues, setIssues] = useState<Issue[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [tabs, setTabs] = useState<TabDef[]>([]);
  const [runs, setRuns] = useState<Run[]>([]);
  const nameRef = useRef<HTMLInputElement>(null);
  const seq = useRef(0);

  useEffect(() => {
    if (open) {
      setDraft(initial);
      setError(null);
    }
  }, [open, initial]);

  useEffect(() => {
    transport.resultsTabs().then(setTabs, () => setTabs([]));
  }, [transport]);

  // The saved schedule's runs, newest first, to tell how many files a lower limit removes.
  useEffect(() => {
    if (!open || !initial || initial.id === "") {
      setRuns([]);
      return;
    }
    let live = true;
    transport.scheduleHistory(initial.id, 1000).then(
      (r) => {
        if (live) setRuns(r);
      },
      () => {
        if (live) setRuns([]);
      },
    );
    return () => {
      live = false;
    };
  }, [transport, open, initial]);

  useEffect(() => {
    if (!draft || !open) return;
    const t = setTimeout(() => {
      const n = ++seq.current;
      void Promise.all([transport.schedulePreview(draft.frequency), transport.scheduleValidate(draft)]).then(([p, v]) => {
        if (n !== seq.current) return;
        setPreview(p);
        setIssues(v);
      });
    }, EDITOR_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [transport, draft, open]);

  if (!draft) return null;
  const set = (patch: Partial<Schedule>) => setDraft((d) => (d ? { ...d, ...patch } : d));
  const isNew = draft.id === "";
  const errors = issues.filter((i) => i.severity === "error");
  const chosen = exportTabs(draft.after);
  const input = draft.input;
  const limit = draft.keep_last;
  const lowered = limit !== null && limit >= 1 && (initial?.keep_last ?? Infinity) > limit;
  const toTrash = lowered ? Math.max(0, runs.filter((r) => r.file !== null).length - limit) : 0;

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      const saved = await transport.scheduleSave(draft);
      onSaved(saved);
      onOpenChange(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Sheet open={open} onOpenChange={onOpenChange} title={isNew ? "New schedule" : "Edit schedule"} className="w-160 sheet-max-w" initialFocus={nameRef}>
      <header className="border-b border-line px-6 py-3">
        <h2 className="m-0 text-md font-semibold">{isNew ? "New schedule" : "Edit schedule"}</h2>
        <p className="m-0 text-sm text-fg-3">
          {draft.source_profile ? `Settings copied from the profile "${draft.source_profile}".` : "Settings copied from the current crawl settings."} Later changes to them do not change this schedule.
        </p>
      </header>
      <form
        className="flex min-h-0 flex-1 flex-col"
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-6 py-4">
          <Group>
            <Row label="Name" issues={issuesAt(issues, "name")}>
              {(id) => <Input id={id} ref={nameRef} className="w-72" value={draft.name} onChange={(e) => set({ name: e.target.value })} />}
            </Row>
            {"spider" in input ? (
              <Row label="Start URL" issues={issuesAt(issues, "input")}>
                {(id) => <Input id={id} mono className="w-72" value={input.spider.start_url} onChange={(e) => set({ input: { spider: { start_url: e.target.value } } })} />}
              </Row>
            ) : (
              <Row label="URL list file" help="Read again at every run, so edits to the file are picked up." issues={issuesAt(issues, "input")}>
                {(id) => (
                  <span className="flex items-center gap-2">
                    <Input id={id} mono className="w-60" value={input.list.path} onChange={(e) => set({ input: { list: { path: e.target.value } } })} />
                    <Button
                      onClick={async () => {
                        const p = await transport.pickListFile();
                        if (p) set({ input: { list: { path: p } } });
                      }}
                    >
                      Choose…
                    </Button>
                  </span>
                )}
              </Row>
            )}
          </Group>

          <Group title="When">
            <FrequencyFields value={draft.frequency} onChange={(frequency) => set({ frequency })} issues={issuesAt(issues, "frequency").filter((i) => i.message !== preview?.error)} />
            <NextRuns preview={preview} />
            <Row label="Missed runs" help="When the computer was off or asleep at a run time: run once at the next login, or record it as Missed.">
              {(_, labelId) => (
                <span className="flex items-center gap-2 text-sm text-fg-2">
                  <span id={labelId}>Catch up</span>
                  <Switch labelledBy={labelId} checked={draft.catch_up} onChange={(catch_up) => set({ catch_up })} />
                </span>
              )}
            </Row>
          </Group>

          <Group title="Results">
            <Row label="Save crawls in" help="Each run is saved as its own file, named by the schedule and the date." issues={issuesAt(issues, "output_dir")}>
              {(id) => (
                <span className="flex items-center gap-2">
                  <Input id={id} mono className="w-60" value={draft.output_dir} onChange={(e) => set({ output_dir: e.target.value })} />
                  <Button
                    onClick={async () => {
                      const p = await transport.pickFolder();
                      if (p) set({ output_dir: p });
                    }}
                  >
                    Choose…
                  </Button>
                </span>
              )}
            </Row>
            <Row
              label="Remove older runs"
              help="Older run files of this schedule go to the Trash. Files you moved or saved elsewhere are never removed."
              issues={issuesAt(issues, "keep_last")}
            >
              {(_, labelId) => <Switch labelledBy={labelId} checked={limit !== null} onChange={(on) => set({ keep_last: on ? DEFAULT_KEEP_LAST : null })} />}
            </Row>
            {limit !== null && (
              <Row label="Runs to keep" help={toTrash > 0 ? `${toTrash} older run ${toTrash === 1 ? "file" : "files"} will go to the Trash at the next cleanup.` : undefined}>
                {(id) => (
                  <NumberInput
                    id={id}
                    className="w-20"
                    min={1}
                    max={999}
                    value={limit}
                    onChange={(v) => set({ keep_last: v === null || Number.isNaN(v) ? 0 : Math.round(v) })}
                  />
                )}
              </Row>
            )}
            <Row label="Export CSV after each run" help="Written to a folder next to the crawl file." stacked>
              {() => (
                <div role="group" aria-label="Tabs to export" className="grid grid-cols-3 gap-x-3 gap-y-1">
                  {tabs.map((t) => (
                    <label key={t.id} className="flex items-center gap-1.5 text-sm text-fg-2">
                      <input
                        type="checkbox"
                        className="size-3.5 accent-accent"
                        checked={chosen.includes(t.id)}
                        onChange={(e) => set({ after: withExportTabs(draft.after, e.target.checked ? [...chosen, t.id] : chosen.filter((x) => x !== t.id)) })}
                      />
                      {t.title}
                    </label>
                  ))}
                </div>
              )}
            </Row>
            <Row label="Compare with the previous run" help="Written as comparison.json next to the crawl file, using the latest earlier run that finished. The crawl report arrives in a later version.">
              {(_, labelId) => (
                <span className="flex items-center gap-2 text-sm text-fg-2">
                  <span id={labelId}>Compare</span>
                  <Switch labelledBy={labelId} checked={hasCompare(draft.after)} onChange={(on) => set({ after: withCompare(draft.after, on) })} />
                </span>
              )}
            </Row>
            <Row label="Notify me">
              {() => <Segmented label="Notify me" value={draft.notify} options={notifyOptions} onChange={(notify) => set({ notify })} />}
            </Row>
          </Group>

          <IssueList issues={issues.filter((i) => !shownAtField.some((f) => issuesAt([i], f).length > 0))} />
          {error && (
            <Notice kind="bad" role="alert">
              {error}
            </Notice>
          )}
        </div>
        <footer className="flex items-center gap-2 border-t border-line px-6 py-3">
          <span className="mr-auto flex items-center gap-2 text-sm text-fg-2">
            <Switch label="Enabled" checked={draft.enabled} onChange={(enabled) => set({ enabled })} />
            {draft.enabled ? "Runs at these times" : "Saved, but off"}
          </span>
          <SheetClose render={<Button />}>Cancel</SheetClose>
          <Button type="submit" variant="primary" disabled={saving || errors.length > 0}>
            {isNew ? "Create schedule" : "Save"}
          </Button>
        </footer>
      </form>
    </Sheet>
  );
}
