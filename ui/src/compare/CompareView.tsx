// The comparison in the main area (spec 012 stories 1, 2, 3, 5): the overview
// of every filter's change, change detection, issue changes, and the URL
// lists behind each number.

import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, Download, X } from "lucide-react";
import type { ElementDelta } from "../generated/ElementDelta";
import type { FilterChange } from "../generated/FilterChange";
import type { FilterDelta } from "../generated/FilterDelta";
import type { Group } from "../generated/Group";
import type { GroupPage } from "../generated/GroupPage";
import type { GroupRow } from "../generated/GroupRow";
import type { IssueChange } from "../generated/IssueChange";
import type { IssueDelta } from "../generated/IssueDelta";
import type { IssuePriority } from "../generated/IssuePriority";
import type { Summary } from "../generated/Summary";
import type { Transport } from "../transport";
import { Badge, Bar, Button, cn, EmptyState, Input, Notice, NumberInput, Segmented, Switch } from "../components/ui";
import { formatCell } from "../results/format";
import { useExport } from "../results/useExport";
import { fileName, phaseLabel, type Comparison } from "./useComparison";

const PAGE = 200;

export type Pair = { prev: number | null; cur: number | null };

/** Change of a filter's count; null when it cannot be compared. */
export const filterChange = (f: FilterDelta) => (f.previous === null || f.current === null ? null : f.current - f.previous);

const signed = (n: number) => (n > 0 ? `+${n.toLocaleString()}` : n.toLocaleString());

function groupTitle(g: Group, s: Summary): string {
  if (g.kind === "urls") return { new: "New URLs", missing: "Missing URLs", not_reached: "Not reached", collisions: "Mapping collisions" }[g.which];
  if (g.kind === "element") return `Changed: ${s.elements.find((e) => e.element === g.element)?.label ?? g.element}`;
  if (g.kind === "issue") {
    const change = { newly_affected: "Newly affected", fixed: "Fixed", gone: "Gone" }[g.change];
    return `${s.issues.find((i) => i.rule === g.rule)?.name ?? g.rule} · ${change}`;
  }
  const f = s.filters.find((x) => x.tab === g.tab && x.filter === g.filter);
  const change = { new: "New", added: "Added", removed: "Removed", missing: "Missing", not_reached: "Not reached" }[g.change];
  return `${f?.tab_title ?? g.tab}: ${f?.filter_label ?? g.filter} · ${change}`;
}

function Stat({ label, value, tone, onOpen }: { label: string; value: number; tone?: "ok" | "warn" | "bad"; onOpen?: () => void }) {
  const body = (
    <>
      <span className="text-xs text-fg-3">{label}</span>
      <span className={cn("text-lg font-semibold tabular", tone === "warn" && "text-warn", tone === "bad" && "text-bad", tone === "ok" && "text-ok")}>{value.toLocaleString()}</span>
    </>
  );
  const cls = "flex min-w-28 flex-col items-start gap-0.5 rounded-lg border border-line bg-surface px-3 py-2 text-left";
  return onOpen && value > 0 ? (
    <button type="button" className={cn(cls, "hover:bg-surface-2")} onClick={onOpen} aria-label={`${label}: ${value.toLocaleString()}`}>
      {body}
    </button>
  ) : (
    <div className={cls}>{body}</div>
  );
}

function Count({ n, label, onOpen }: { n: number; label: string; onOpen: () => void }) {
  if (n === 0) return <span className="text-fg-3 tabular">0</span>;
  return (
    <button type="button" className="rounded-sm text-accent-text tabular underline-offset-2 hover:underline" aria-label={label} onClick={onOpen}>
      {n.toLocaleString()}
    </button>
  );
}

function Overview({ summary, onOpen }: { summary: Summary; onOpen: (g: Group) => void }) {
  const [hideUnchanged, setHideUnchanged] = useState(true);
  const [sort, setSort] = useState<"change" | "name">("change");
  const rows = useMemo(() => {
    const moved = (f: FilterDelta) => f.new + f.added + f.removed + f.missing + f.not_reached > 0 || (filterChange(f) ?? 0) !== 0;
    const list = summary.filters.filter((f) => !hideUnchanged || moved(f) || !f.comparable);
    return sort === "name"
      ? list
      : [...list].sort((a, b) => Math.abs(filterChange(b) ?? -1) - Math.abs(filterChange(a) ?? -1));
  }, [summary, hideUnchanged, sort]);
  const open = (f: FilterDelta, change: FilterChange) => () => onOpen({ kind: "filter", tab: f.tab, filter: f.filter, change });
  const p = summary.previous;
  const c = summary.current;
  return (
    <div className="flex flex-col gap-4 p-4">
      <div className="flex flex-wrap gap-2">
        <Stat label="Matched" value={summary.matched} />
        <Stat label="New URLs" value={summary.new} onOpen={() => onOpen({ kind: "urls", which: "new" })} />
        <Stat label="Missing URLs" value={summary.missing} tone={summary.missing > 0 ? "warn" : undefined} onOpen={() => onOpen({ kind: "urls", which: "missing" })} />
        {summary.not_reached > 0 && <Stat label="Not reached" value={summary.not_reached} onOpen={() => onOpen({ kind: "urls", which: "not_reached" })} />}
        <Stat label="Lost indexable pages" value={summary.lost_indexable} tone={summary.lost_indexable > 0 ? "bad" : undefined} />
        {summary.collisions > 0 && <Stat label="Mapping collisions" value={summary.collisions} tone="warn" onOpen={() => onOpen({ kind: "urls", which: "collisions" })} />}
      </div>

      <div className="flex flex-col gap-1">
        {summary.low_overlap && (
          <Notice kind="warn" role="alert">
            Few URLs match between the two crawls. If the site moved, add a URL mapping.
          </Notice>
        )}
        {!c.complete && (
          <Notice kind="warn">The current crawl did not finish; URLs it did not reach are listed as Not reached, not Missing.</Notice>
        )}
        {(p.snapshot || c.snapshot) && <Notice kind="info">A crawl was still running; the comparison uses what it had saved.</Notice>}
        {summary.config_diffs.length > 0 && (
          <Notice kind="warn">
            Different settings: {summary.config_diffs.map((d) => `${d.label} ${d.previous} → ${d.current}`).join("; ")}
          </Notice>
        )}
      </div>

      <div className="flex items-center gap-3">
        <h3 className="m-0 text-sm font-semibold">Filters</h3>
        <label className="flex items-center gap-1.5 text-sm text-fg-2">
          <Switch label="Hide unchanged" checked={hideUnchanged} onChange={setHideUnchanged} />
          Hide unchanged
        </label>
        <Segmented
          size="sm"
          label="Sort filters"
          value={sort}
          onChange={setSort}
          options={[
            { value: "change", label: "Largest change" },
            { value: "name", label: "Tab order" },
          ]}
        />
      </div>
      <table aria-label="Filter changes" className="w-full border-collapse text-sm">
        <thead>
          <tr className="border-b border-line text-left text-xs text-fg-3">
            <th className="py-1 pr-2 font-semibold">Filter</th>
            {["Previous", "Current", "Change", "New", "Added", "Removed", "Missing"].map((h) => (
              <th key={h} className="py-1 pr-2 text-right font-semibold">
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((f) => {
            const name = `${f.tab_title}: ${f.filter_label}`;
            const d = filterChange(f);
            return (
              <tr key={`${f.tab}/${f.filter}`} className="border-b border-line/60">
                <td className="py-1 pr-2">
                  {name}
                  {!f.comparable && f.reason && <div className="text-xs text-fg-3">{f.reason}</div>}
                </td>
                <td className="py-1 pr-2 text-right tabular">{f.previous?.toLocaleString() ?? "–"}</td>
                <td className="py-1 pr-2 text-right tabular">{f.current?.toLocaleString() ?? "–"}</td>
                <td className={cn("py-1 pr-2 text-right font-semibold tabular", d !== null && d > 0 && "text-warn", d !== null && d < 0 && "text-ok")}>{d === null ? "–" : signed(d)}</td>
                <td className="py-1 pr-2 text-right">{f.comparable && <Count n={f.new} label={`${name}: new`} onOpen={open(f, "new")} />}</td>
                <td className="py-1 pr-2 text-right">{f.comparable && <Count n={f.added} label={`${name}: added`} onOpen={open(f, "added")} />}</td>
                <td className="py-1 pr-2 text-right">{f.comparable && <Count n={f.removed} label={`${name}: removed`} onOpen={open(f, "removed")} />}</td>
                <td className="py-1 pr-2 text-right">{f.comparable && <Count n={f.missing} label={`${name}: missing`} onOpen={open(f, "missing")} />}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
      {rows.length === 0 && <p className="m-0 text-sm text-fg-3">No filter changed between the two crawls.</p>}
    </div>
  );
}

const thresholdUnit: Partial<Record<ElementDelta["kind"], string>> = { percent: "%", absolute: "levels", factor: "× slower or faster" };

function ChangeDetection({ elements, onChange, onOpen }: { elements: ElementDelta[]; onChange: (e: ElementDelta[]) => void; onOpen: (g: Group) => void }) {
  const set = (id: string, patch: Partial<ElementDelta>) => onChange(elements.map((e) => (e.element === id ? { ...e, ...patch } : e)));
  return (
    <div className="flex flex-col gap-2 p-4">
      <p className="m-0 text-sm text-fg-2">URLs in both crawls whose value changed. Thresholds apply at once, without reading the crawls again.</p>
      <table aria-label="Change detection" className="w-full border-collapse text-sm">
        <thead>
          <tr className="border-b border-line text-left text-xs text-fg-3">
            <th className="py-1 pr-2 font-semibold">Element</th>
            <th className="py-1 pr-2 font-semibold">Tracked</th>
            <th className="py-1 pr-2 font-semibold">Threshold</th>
            <th className="py-1 pr-2 text-right font-semibold">Changed URLs</th>
          </tr>
        </thead>
        <tbody>
          {elements.map((e) => (
            <tr key={e.element} className="border-b border-line/60">
              <td className="py-1 pr-2">
                {e.label}
                {!e.comparable && e.reason && <div className="text-xs text-fg-3">{e.reason}</div>}
              </td>
              <td className="py-1 pr-2">
                <Switch label={`Track ${e.label}`} checked={e.on} onChange={(on) => set(e.element, { on })} />
              </td>
              <td className="py-1 pr-2">
                {e.threshold !== null && thresholdUnit[e.kind] && (
                  <span className="flex items-center gap-1.5">
                    <NumberInput
                      aria-label={`${e.label} threshold`}
                      value={e.threshold}
                      min={0}
                      disabled={!e.on}
                      onChange={(v) => v !== null && !Number.isNaN(v) && v >= 0 && set(e.element, { threshold: v })}
                    />
                    <span className="text-fg-3">{thresholdUnit[e.kind]}</span>
                  </span>
                )}
              </td>
              <td className="py-1 pr-2 text-right">
                {e.on && e.comparable ? <Count n={e.changed} label={`${e.label}: changed`} onOpen={() => onOpen({ kind: "element", element: e.element })} /> : <span className="text-fg-3">–</span>}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

const PRIORITIES: { id: IssuePriority; label: string }[] = [
  { id: "high", label: "High" },
  { id: "medium", label: "Medium" },
  { id: "low", label: "Low" },
  { id: "opportunity", label: "Opportunity" },
];

const settingsOwner = { current: "the current crawl's", previous: "the previous crawl's", app: "the app's" } as const;

/** Change of an issue's affected URLs; null when it cannot be compared. */
export const issueChange = (i: IssueDelta) => (i.previous === null || i.current === null ? null : i.current - i.previous);

function Issues({ summary, onOpen }: { summary: Summary; onOpen: (g: Group) => void }) {
  const [show, setShow] = useState<"all" | "regressions" | "improvements">("all");
  const basis = summary.issue_basis;
  const open = (i: IssueDelta, change: IssueChange) => () => onOpen({ kind: "issue", rule: i.rule, change });
  const shown = summary.issues.filter((i) => show === "all" || !i.comparable || (show === "regressions" ? i.newly_affected > 0 : i.fixed > 0));
  return (
    <div className="flex flex-col gap-3 p-4">
      <p className="m-0 text-sm text-fg-2">
        Both crawls evaluated with rule set v{basis.rules_version} and {settingsOwner[basis.settings]} issue settings{basis.custom ? " (customised)" : ""}.
      </p>
      <Segmented
        size="sm"
        label="Show issues"
        value={show}
        onChange={setShow}
        options={[
          { value: "all", label: "All" },
          { value: "regressions", label: "Only regressions" },
          { value: "improvements", label: "Only improvements" },
        ]}
      />
      <table aria-label="Issue changes" className="w-full border-collapse text-sm">
        <thead>
          <tr className="border-b border-line text-left text-xs text-fg-3">
            <th className="py-1 pr-2 font-semibold">Issue</th>
            {["Previous", "Current", "Change", "Newly affected", "Fixed", "Gone"].map((h) => (
              <th key={h} className="py-1 pr-2 text-right font-semibold">
                {h}
              </th>
            ))}
          </tr>
        </thead>
        {PRIORITIES.map((p) => {
          const rows = shown.filter((i) => i.priority === p.id);
          if (rows.length === 0) return null;
          return (
            <tbody key={p.id} aria-label={`${p.label} priority`}>
              <tr>
                <th colSpan={7} scope="colgroup" className="pt-3 pb-1 text-left text-xs font-semibold text-fg-2">
                  {p.label}
                </th>
              </tr>
              {rows.map((i) => {
                const d = issueChange(i);
                return (
                  <tr key={i.rule} className={cn("border-b border-line/60", !i.comparable && "text-fg-3")}>
                    <td className="py-1 pr-2">
                      {i.name}
                      {!i.comparable && i.reason && <div className="text-xs">{i.reason}</div>}
                    </td>
                    <td className="py-1 pr-2 text-right tabular">{i.previous?.toLocaleString() ?? "–"}</td>
                    <td className="py-1 pr-2 text-right tabular">{i.current?.toLocaleString() ?? "–"}</td>
                    <td className={cn("py-1 pr-2 text-right font-semibold tabular", d !== null && d > 0 && "text-warn", d !== null && d < 0 && "text-ok")}>{d === null ? "–" : signed(d)}</td>
                    <td className="py-1 pr-2 text-right">{i.comparable && <Count n={i.newly_affected} label={`${i.name}: newly affected`} onOpen={open(i, "newly_affected")} />}</td>
                    <td className="py-1 pr-2 text-right">{i.comparable && <Count n={i.fixed} label={`${i.name}: fixed`} onOpen={open(i, "fixed")} />}</td>
                    <td className="py-1 pr-2 text-right">{i.comparable && <Count n={i.gone} label={`${i.name}: gone`} onOpen={open(i, "gone")} />}</td>
                  </tr>
                );
              })}
            </tbody>
          );
        })}
      </table>
      {shown.length === 0 && <p className="m-0 text-sm text-fg-3">{show === "all" ? "No issue rules were evaluated." : "No issue matches."}</p>}
    </div>
  );
}

function GroupList({
  transport,
  id,
  group,
  title,
  selected,
  onSelect,
  onBack,
}: {
  transport: Transport;
  id: number;
  group: Group;
  title: string;
  selected: Pair | null;
  onSelect: (p: Pair) => void;
  onBack: () => void;
}) {
  const [search, setSearch] = useState("");
  const [page, setPage] = useState<GroupPage | null>(null);
  const [rows, setRows] = useState<GroupRow[]>([]);
  const [error, setError] = useState<string | null>(null);
  const exp = useExport(transport);

  useEffect(() => {
    let off = false;
    let view: number | null = null;
    const t = setTimeout(() => {
      transport.compareOpenGroup(id, group, search, null, PAGE).then(
        (p) => {
          view = p.view;
          if (off) return void transport.compareCloseGroup(id, p.view);
          setPage(p);
          setRows(p.rows);
          setError(null);
        },
        (e) => !off && setError(String(e)),
      );
    }, search ? 200 : 0);
    return () => {
      off = true;
      clearTimeout(t);
      if (view !== null) void transport.compareCloseGroup(id, view);
    };
  }, [transport, id, group, search]);

  const more = async () => {
    if (!page) return;
    const p = await transport.compareRows(id, page.view, rows.length, PAGE);
    setRows((r) => [...r, ...p.rows]);
  };
  const exportCsv = async () => {
    if (!page) return;
    const dest = await transport.pickCsvPath(title.replace(/[^\w-]+/g, "-").toLowerCase());
    if (dest) void exp.run(() => transport.compareExport(id, page.view, dest), dest);
  };

  return (
    <section aria-label={title} className="flex h-full min-h-0 flex-col">
      <header className="flex items-center gap-2 border-b border-line px-3 py-2">
        <Button size="sm" variant="quiet" aria-label="Back to the comparison overview" onClick={onBack}>
          <ArrowLeft aria-hidden className="size-3.5" /> Overview
        </Button>
        <h3 className="m-0 min-w-0 truncate text-base font-semibold">{title}</h3>
        <span data-testid="group-count" className="text-sm text-fg-3 tabular">
          {page ? `${page.total.toLocaleString()} URLs` : ""}
        </span>
        <Input aria-label="Search addresses" className="ml-auto w-56" placeholder="Search addresses" value={search} onChange={(e) => setSearch(e.target.value)} />
        <Button size="sm" disabled={!page || page.total === 0 || exp.job !== null} onClick={() => void exportCsv()}>
          <Download aria-hidden className="size-3.5" /> Export
        </Button>
      </header>
      {error && (
        <Notice kind="bad" role="alert" className="px-3 py-2">
          {error}
        </Notice>
      )}
      <div className="min-h-0 flex-1 overflow-auto">
        {page && (
          <table role="grid" aria-label={`${title} URLs`} className="w-max min-w-full border-collapse text-sm">
            <thead className="sticky top-0 bg-chrome">
              <tr className="border-b border-line text-left text-xs text-fg-3">
                {page.columns.map((c) => (
                  <th key={c.id} className="px-2 py-1 font-semibold" style={{ width: c.width }}>
                    {c.label}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((r, i) => {
                const on = selected !== null && selected.prev === r.prev_id && selected.cur === r.cur_id;
                return (
                  <tr
                    key={`${r.prev_id}-${r.cur_id}-${i}`}
                    aria-selected={on}
                    tabIndex={0}
                    onClick={() => onSelect({ prev: r.prev_id, cur: r.cur_id })}
                    onKeyDown={(e) => e.key === "Enter" && onSelect({ prev: r.prev_id, cur: r.cur_id })}
                    className={cn("cursor-default border-b border-line/60 outline-none focus-visible:bg-surface-2", on ? "bg-selection" : "hover:bg-surface-2")}
                  >
                    {page.columns.map((c, k) => (
                      <td key={c.id} className="max-w-96 truncate px-2 py-1" title={String(r.values[k] ?? "")}>
                        {formatCell(c.kind, r.values[k] ?? null)}
                      </td>
                    ))}
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
        {page && rows.length < page.total && (
          <div className="p-3">
            <Button size="sm" onClick={() => void more()}>
              Show more
            </Button>
          </div>
        )}
        {page && page.total === 0 && <EmptyState title="No URLs">{search ? "Nothing matches the search." : "This group is empty."}</EmptyState>}
      </div>
      {(exp.job !== null || exp.result) && (
        <footer className="flex h-8 shrink-0 items-center gap-3 border-t border-line bg-chrome px-3 text-sm">
          {exp.job !== null && <span className="text-fg-2">Exporting…</span>}
          {exp.job === null && exp.result && (
            <Notice kind={exp.result.state === "failed" ? "bad" : "info"} role="status">
              {exp.result.state === "done"
                ? `Exported ${exp.result.rows_written.toLocaleString()} rows to ${exp.result.path}`
                : exp.result.state === "cancelled"
                  ? "Export cancelled; no file was written."
                  : `Export failed: ${exp.result.error}`}
            </Notice>
          )}
        </footer>
      )}
    </section>
  );
}

/** The comparison screen: running, failed, or the result. */
export function CompareView({
  transport,
  comparison,
  selected,
  onSelect,
  onNew,
}: {
  transport: Transport;
  comparison: Comparison;
  selected: Pair | null;
  onSelect: (p: Pair | null) => void;
  onNew: () => void;
}) {
  const s = comparison.state;
  const [tab, setTab] = useState<"overview" | "elements" | "issues">("overview");
  const [group, setGroup] = useState<Group | null>(null);
  const openGroup = (g: Group) => {
    onSelect(null);
    setGroup(g);
  };

  if (s.phase === "idle") {
    return (
      <EmptyState title="No comparison open">
        <Button onClick={onNew}>Compare crawls…</Button>
      </EmptyState>
    );
  }
  if (s.phase === "failed") {
    return (
      <section aria-label="Comparison" className="flex h-full flex-col items-center justify-center gap-3 p-6">
        <Notice kind="bad" role="alert">
          {s.error}
        </Notice>
        <Button onClick={onNew}>Compare crawls…</Button>
      </section>
    );
  }
  if (s.phase === "running") {
    const p = s.progress;
    return (
      <section aria-label="Comparison" className="flex h-full flex-col items-center justify-center gap-3 p-6">
        <div className="text-base font-semibold">
          Comparing {fileName(s.previous)} with {fileName(s.current)}
        </div>
        <div role="status" className="text-sm text-fg-2">
          {p ? phaseLabel[p.phase] : phaseLabel.open}
          {p && p.total > 1 ? ` · ${p.done.toLocaleString()} of ${p.total.toLocaleString()} URLs` : ""}
        </div>
        <Bar value={p && p.total ? p.done / p.total : 0} tone="accent" label="Comparison progress" className="w-64" />
        <Button size="sm" onClick={comparison.cancel}>
          Cancel
        </Button>
      </section>
    );
  }

  const summary = s.summary;
  return (
    <section aria-label="Comparison" className="flex h-full min-h-0 flex-col">
      <header className="flex items-center gap-3 border-b border-line px-3 py-2">
        <div className="min-w-0">
          <div className="truncate text-base font-semibold">
            {fileName(summary.previous.path)} → {fileName(summary.current.path)}
          </div>
          <div className="truncate text-xs text-fg-3">{summary.current.start_url}</div>
        </div>
        {s.settings.mapping.length > 0 && <Badge tone="info">{s.settings.mapping.length === 1 ? "1 mapping rule" : `${s.settings.mapping.length} mapping rules`}</Badge>}
        <div className="ml-auto flex items-center gap-2">
          {!group && (
            <Segmented
              size="sm"
              label="Comparison view"
              value={tab}
              onChange={setTab}
              options={[
                { value: "overview", label: "Overview" },
                { value: "elements", label: "Change detection" },
                { value: "issues", label: "Issues" },
              ]}
            />
          )}
          <Button size="sm" variant="quiet" onClick={onNew}>
            New comparison…
          </Button>
          <Button size="sm" icon variant="quiet" aria-label="Close comparison" onClick={() => void comparison.close()}>
            <X aria-hidden className="size-3.5" />
          </Button>
        </div>
      </header>
      <div className="min-h-0 flex-1 overflow-auto">
        {group ? (
          <GroupList
            transport={transport}
            id={s.id}
            group={group}
            title={groupTitle(group, summary)}
            selected={selected}
            onSelect={onSelect}
            onBack={() => {
              setGroup(null);
              onSelect(null);
            }}
          />
        ) : tab === "overview" ? (
          <Overview summary={summary} onOpen={openGroup} />
        ) : tab === "issues" ? (
          <Issues summary={summary} onOpen={openGroup} />
        ) : (
          <ChangeDetection
            elements={summary.elements}
            onOpen={openGroup}
            onChange={(els) => void comparison.setElements(els.map((e) => ({ id: e.element, on: e.on, threshold: e.threshold })))}
          />
        )}
      </div>
    </section>
  );
}
