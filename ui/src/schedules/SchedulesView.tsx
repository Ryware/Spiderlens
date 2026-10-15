import { useCallback, useEffect, useState } from "react";
import { CalendarClock, Play, Plus, Square } from "lucide-react";
import type { Run } from "../generated/Run";
import type { Schedule } from "../generated/Schedule";
import type { ScheduleDetail } from "../generated/ScheduleDetail";
import type { ScheduleRow } from "../generated/ScheduleRow";
import type { ScheduleSaved, Transport } from "../transport";
import { Badge, Button, cn, EmptyState, Input, Menu, Modal, Notice, Switch } from "../components/ui";
import { fileName, formatDuration, formatRunTime, registrationNote, runStatusInfo, triggerLabel } from "./format";
import type { Schedules } from "./useSchedules";

/** Runs shown in the history; the CLI shows more with --limit. */
export const HISTORY_LIMIT = 50;

const count = (n: number | undefined) => (n ?? 0).toLocaleString();

function RunBadge({ run }: { run: Run }) {
  const s = runStatusInfo[run.status];
  return <Badge tone={s.tone}>{s.label}</Badge>;
}

function ListRow({ row, selected, onSelect }: { row: ScheduleRow; selected: boolean; onSelect: () => void }) {
  const s = row.schedule;
  const reg = registrationNote(row.registration);
  return (
    <li data-id={s.id}>
      <button
        type="button"
        aria-current={selected || undefined}
        onClick={onSelect}
        className={cn("flex w-full flex-col gap-0.5 px-3 py-2 text-left focus-visible:outline-none", selected ? "bg-selection" : "hover:bg-surface-2 focus-visible:bg-surface-2")}
      >
        <span className="flex items-center gap-2">
          <span className="min-w-0 flex-1 truncate text-base font-medium text-fg">{s.name}</span>
          {row.running ? <Badge tone="info">Running</Badge> : row.last && <RunBadge run={row.last} />}
          {reg && <Badge tone={reg.tone}>{reg.label}</Badge>}
        </span>
        <span className="truncate text-sm text-fg-3">{row.describe}</span>
        <span className="text-xs text-fg-3 tabular">{row.running ? `${count(row.running.counts.crawled)} crawled so far` : row.next_run ? `Next: ${formatRunTime(row.next_run)}` : "No upcoming runs"}</span>
      </button>
    </li>
  );
}

function History({ runs, onOpenCrawl }: { runs: Run[]; onOpenCrawl: (path: string) => void }) {
  if (runs.length === 0) return <p className="m-0 px-1 text-sm text-fg-3">No runs yet.</p>;
  return (
    <table aria-label="Run history" className="w-full border-collapse text-sm">
      <thead>
        <tr className="text-left text-xs text-fg-3">
          <th className="px-2 py-1 font-semibold">Run</th>
          <th className="px-2 py-1 font-semibold">Status</th>
          <th className="px-2 py-1 font-semibold">Started</th>
          <th className="px-2 py-1 font-semibold">Duration</th>
          <th className="px-2 py-1 text-right font-semibold">Crawled</th>
          <th className="px-2 py-1 font-semibold">File</th>
        </tr>
      </thead>
      <tbody>
        {runs.map((r) => (
          <tr key={r.run} data-run={r.run} className="border-t border-line align-top">
            <td className="px-2 py-1.5 text-fg-2 tabular">
              #{r.run}
              <div className="text-xs text-fg-3">{triggerLabel[r.trigger]}</div>
            </td>
            <td className="px-2 py-1.5">
              <RunBadge run={r} />
              {r.reason && <div className="mt-0.5 max-w-64 text-xs text-fg-2">{r.reason}</div>}
              {r.steps.map((st) => (
                <div key={st.kind} data-step={st.kind} className={cn("mt-0.5 text-xs", st.status === "failed" ? "text-bad" : "text-fg-3")}>
                  {st.kind === "export" ? "CSV export" : st.kind === "report" ? "Report" : "Comparison"}: {st.status === "done" ? `${st.outputs.length} files` : st.status}
                  {st.reason ? ` · ${st.reason}` : ""}
                </div>
              ))}
            </td>
            <td className="px-2 py-1.5 whitespace-nowrap text-fg-2 tabular">{formatRunTime(r.started_at ?? r.slot)}</td>
            <td className="px-2 py-1.5 whitespace-nowrap text-fg-2 tabular">{formatDuration(r.started_at, r.ended_at)}</td>
            <td className="px-2 py-1.5 text-right text-fg-2 tabular">{r.counts ? count(r.counts.crawled) : "–"}</td>
            <td className="px-2 py-1.5">
              {r.file ? (
                <button type="button" title={r.file} onClick={() => onOpenCrawl(r.file!)} className="max-w-56 truncate text-left text-accent-text hover:underline">
                  {fileName(r.file)}
                </button>
              ) : (
                <span className="text-fg-3">–</span>
              )}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function DetailPane({
  transport,
  id,
  version,
  onChanged,
  onEdit,
  onOpenCrawl,
}: {
  transport: Transport;
  id: string;
  /** Changes when the detail must reload: a save, a new run, new live counts. */
  version: string;
  onChanged: (removed?: boolean) => void;
  onEdit: (s: Schedule) => void;
  onOpenCrawl: (path: string) => void;
}) {
  const [detail, setDetail] = useState<ScheduleDetail | null>(null);
  const [runs, setRuns] = useState<Run[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<"delete" | "duplicate" | null>(null);
  const [copyName, setCopyName] = useState("");

  const load = useCallback(async () => {
    try {
      const [d, h] = await Promise.all([transport.scheduleGet(id), transport.scheduleHistory(id, HISTORY_LIMIT)]);
      setDetail(d);
      setRuns(h);
    } catch (e) {
      setError(String(e));
    }
  }, [transport, id]);

  useEffect(() => {
    void load();
  }, [load, version]);

  if (!detail) return error ? <Notice kind="bad" role="alert" className="p-4">{error}</Notice> : null;
  const row = detail.row;
  const s = row.schedule;
  const reg = registrationNote(row.registration);

  const act = (f: () => Promise<unknown>, after?: (v: unknown) => void) => async () => {
    setError(null);
    setNotice(null);
    try {
      const v = await f();
      after?.(v);
      onChanged();
      await load();
    } catch (e) {
      setError(String(e));
    }
  };
  const fromSaved = (v: unknown) => {
    const r = v as ScheduleSaved;
    if (r.refused) setError(`The system scheduler refused this schedule: ${r.refused}`);
  };

  return (
    <section aria-label={s.name} className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-5">
      <header className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <h2 className="m-0 truncate text-lg font-semibold">{s.name}</h2>
          <p className="m-0 text-sm text-fg-2">
            {"spider" in s.input ? s.input.spider.start_url : `List: ${s.input.list.path}`} · {row.describe}
          </p>
        </div>
        <span className="flex items-center gap-2 text-sm text-fg-2">
          <Switch label="Enabled" checked={s.enabled} onChange={(on) => void act(() => transport.scheduleSetEnabled(s.id, on), fromSaved)()} />
          {s.enabled ? "On" : "Off"}
        </span>
      </header>

      <div className="flex flex-wrap items-center gap-2">
        {row.running ? (
          <>
            <Button variant="destructive" onClick={act(() => transport.scheduleStop(s.id), () => setNotice("Stopping the run…"))}>
              <Square aria-hidden className="size-3.5" /> Stop
            </Button>
            <Button onClick={() => onOpenCrawl(row.running!.file)}>Watch</Button>
          </>
        ) : (
          <Button variant="primary" onClick={act(() => transport.scheduleRunNow(s.id), () => setNotice("Started. The run appears here in a moment."))}>
            <Play aria-hidden className="size-3.5" /> Run now
          </Button>
        )}
        <Button onClick={() => onEdit(s)}>Edit…</Button>
        <Menu
          label="More actions"
          trigger={<Button>More</Button>}
          items={[
            {
              label: "Duplicate…",
              onSelect: () => {
                setCopyName(`${s.name} copy`);
                setConfirm("duplicate");
              },
            },
            null,
            { label: "Delete…", destructive: true, onSelect: () => setConfirm("delete") },
          ]}
        />
      </div>

      {row.running && (
        <div role="status" className="rounded-lg border border-line bg-surface px-3 py-2 text-sm text-fg-2 tabular">
          Run #{row.running.run} is running: {count(row.running.counts.crawled)} crawled, {count(row.running.counts.discovered)} found, {count(row.running.counts.failed)} failed.
        </div>
      )}
      {reg?.detail && (
        <Notice kind="bad" role="alert">
          Not registered with the system scheduler: {reg.detail}
        </Notice>
      )}
      {detail.profile_changed && s.source_profile && (
        <div className="flex items-center gap-3 rounded-lg border border-line bg-surface px-3 py-2">
          <Notice kind="info" className="flex-1">
            The profile "{s.source_profile}" changed since this schedule copied it.
          </Notice>
          <Button size="sm" onClick={act(() => transport.scheduleUpdateFromProfile(s.id), fromSaved)}>
            Update from profile
          </Button>
        </div>
      )}
      {notice && <Notice role="status">{notice}</Notice>}
      {error && (
        <Notice kind="bad" role="alert">
          {error}
        </Notice>
      )}

      <section aria-labelledby="next-runs" className="flex flex-col gap-1">
        <h3 id="next-runs" className="m-0 px-1 text-xs font-semibold text-fg-3">
          Next runs
        </h3>
        {detail.next_runs.length ? (
          <ol aria-label="Next runs" className="m-0 flex list-none flex-wrap gap-x-4 gap-y-0.5 px-1 text-sm text-fg-2 tabular">
            {detail.next_runs.map((t) => (
              <li key={t}>{formatRunTime(t)}</li>
            ))}
          </ol>
        ) : (
          <p className="m-0 px-1 text-sm text-fg-3">{s.enabled ? "None" : "Off: turn it on to run at its times."}</p>
        )}
        <p className="m-0 px-1 text-xs text-fg-3">Saved in {s.output_dir}</p>
      </section>

      <section aria-labelledby="history" className="flex flex-col gap-1">
        <h3 id="history" className="m-0 px-1 text-xs font-semibold text-fg-3">
          History
        </h3>
        <History runs={runs} onOpenCrawl={onOpenCrawl} />
      </section>

      <Modal open={confirm === "delete"} onOpenChange={(o) => !o && setConfirm(null)} title={`Delete "${s.name}"?`} description="Its crawl files and exports stay where they are. The system scheduler entry is removed.">
        <div className="flex justify-end gap-2">
          <Button onClick={() => setConfirm(null)}>Cancel</Button>
          <Button
            variant="destructive"
            onClick={async () => {
              try {
                await transport.scheduleDelete(s.id);
                setConfirm(null);
                onChanged(true);
              } catch (e) {
                setError(String(e));
                setConfirm(null);
              }
            }}
          >
            Delete schedule
          </Button>
        </div>
      </Modal>
      <Modal open={confirm === "duplicate"} onOpenChange={(o) => !o && setConfirm(null)} title="Duplicate schedule" description="The copy starts off; turn it on when it is ready.">
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            setConfirm(null);
            void act(() => transport.scheduleDuplicate(s.id, copyName))();
          }}
        >
          <Input aria-label="Name of the copy" value={copyName} onChange={(e) => setCopyName(e.target.value)} autoFocus />
          <div className="flex justify-end gap-2">
            <Button onClick={() => setConfirm(null)}>Cancel</Button>
            <Button type="submit" variant="primary" disabled={!copyName.trim()}>
              Duplicate
            </Button>
          </div>
        </form>
      </Modal>
    </section>
  );
}

/**
 * Scheduled crawls (spec 021 US1, US2, US5): the list with next run and last
 * status on the left, the chosen schedule with its history on the right.
 */
export function SchedulesView({
  transport,
  schedules,
  version,
  onNew,
  onEdit,
  onOpenCrawl,
}: {
  transport: Transport;
  schedules: Schedules;
  /** Bumped by the shell after the editor saves, so the detail reloads. */
  version: number;
  onNew: () => void;
  onEdit: (s: Schedule) => void;
  onOpenCrawl: (path: string) => void;
}) {
  const { rows, error, reload, repair, dismissRepair } = schedules;
  const [selected, setSelected] = useState<string | null>(null);
  const [removeAll, setRemoveAll] = useState(false);
  const current = rows?.find((r) => r.schedule.id === selected) ?? rows?.[0] ?? null;
  // Live counts in the detail follow the list's refresh.
  const live = current?.running ? current.running.counts.crawled : -1;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="flex items-center gap-2 border-b border-line px-4 py-2.5">
        <CalendarClock aria-hidden className="size-4 text-fg-3" />
        <h1 className="m-0 flex-1 text-md font-semibold">Schedules</h1>
        {rows && rows.length > 0 && (
          <Button variant="quiet" size="sm" onClick={() => setRemoveAll(true)}>
            Remove all…
          </Button>
        )}
        <Button variant="primary" size="sm" onClick={onNew}>
          <Plus aria-hidden className="size-3.5" /> New schedule
        </Button>
      </header>
      {repair && (
        <div role="status" className="flex items-start gap-3 border-b border-line bg-surface px-4 py-2 text-sm">
          <div className="flex-1">
            {repair.repaired.length > 0 && <Notice>Updated the system scheduler for: {repair.repaired.join(", ")}.</Notice>}
            {repair.failed.map(([name, reason]) => (
              <Notice key={name} kind="bad">
                Could not register {name}: {reason}
              </Notice>
            ))}
          </div>
          <Button size="sm" variant="quiet" onClick={dismissRepair}>
            Dismiss
          </Button>
        </div>
      )}
      {error && (
        <Notice kind="bad" role="alert" className="px-4 py-2">
          {error}
        </Notice>
      )}
      {rows && rows.length === 0 ? (
        <EmptyState icon={<CalendarClock className="size-6" />} title="No schedules yet">
          Crawl a site every day, week or month, even when SpiderLens is closed. Each run is saved as its own file.
          <div className="mt-3">
            <Button variant="primary" onClick={onNew}>
              New schedule
            </Button>
          </div>
        </EmptyState>
      ) : (
        <div className="flex min-h-0 flex-1">
          <ul aria-label="Schedules" className="m-0 w-72 shrink-0 list-none divide-y divide-line overflow-y-auto border-r border-line p-0">
            {rows?.map((r) => (
              <ListRow key={r.schedule.id} row={r} selected={r.schedule.id === current?.schedule.id} onSelect={() => setSelected(r.schedule.id)} />
            ))}
          </ul>
          {current && (
            <DetailPane
              key={current.schedule.id}
              transport={transport}
              id={current.schedule.id}
              version={`${version}:${live}:${current.last?.run ?? 0}`}
              onChanged={(removed) => {
                if (removed) setSelected(null);
                void reload();
              }}
              onEdit={onEdit}
              onOpenCrawl={onOpenCrawl}
            />
          )}
        </div>
      )}
      <Modal open={removeAll} onOpenChange={setRemoveAll} title="Remove all schedules?" description="Every schedule and its system scheduler entry is removed. Crawl files stay where they are.">
        <div className="flex justify-end gap-2">
          <Button onClick={() => setRemoveAll(false)}>Cancel</Button>
          <Button
            variant="destructive"
            onClick={async () => {
              setRemoveAll(false);
              await transport.schedulesRemoveAll().catch(() => {});
              setSelected(null);
              void reload();
            }}
          >
            Remove all
          </Button>
        </div>
      </Modal>
    </div>
  );
}
