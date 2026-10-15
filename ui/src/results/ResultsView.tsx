// One results tab (US1-US4): filters with counts, scope, search, columns,
// the table, copy and export.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Download, Search, SearchX } from "lucide-react";
import type { Features } from "../generated/Features";
import type { FilterCounts } from "../generated/FilterCounts";
import type { Need } from "../generated/Need";
import type { TabDef } from "../generated/TabDef";
import type { UrlScope } from "../generated/UrlScope";
import type { ViewQuery } from "../generated/ViewQuery";
import type { Transport } from "../transport";
import { Bar, Button, EmptyState, Input, Menu, Notice, Segmented } from "../components/ui";
import { AiOverview } from "./AiOverview";
import { ColumnMenu } from "./ColumnMenu";
import { defaultPrefs, loadPrefs, savePrefs, type ColumnPrefs } from "./columnPrefs";
import { FilterStrip } from "./FilterStrip";
import { Grid, type GridColumn, type GridHandle } from "./Grid";
import { count, emptySelection, shift, single, type Selection } from "./selection";
import type { TabSession } from "./useResults";
import { useExport } from "./useExport";
import { useView } from "./useView";

/** Most rows copied through the clipboard (spec Assumptions); export does more. */
export const COPY_LIMIT = 10_000;

function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setV(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return v;
}

/** Filter counts, refreshed as the view changes but at most every 2 s. */
function useCounts(transport: Transport, path: string, tab: string, scope: UrlScope, version: number) {
  const [counts, setCounts] = useState<FilterCounts | null>(null);
  const last = useRef(0);
  const busy = useRef(false);
  const [again, setAgain] = useState(0);
  useEffect(() => {
    setCounts(null);
    last.current = 0;
  }, [path, tab, scope]);
  useEffect(() => {
    if (busy.current) return;
    const wait = last.current + 2000 - Date.now();
    if (wait > 0) {
      const t = setTimeout(() => setAgain((n) => n + 1), wait);
      return () => clearTimeout(t);
    }
    busy.current = true;
    last.current = Date.now();
    transport
      .viewCounts(path, tab, scope)
      .then(setCounts, () => {})
      .finally(() => (busy.current = false));
  }, [transport, path, tab, scope, version, again]);
  return counts;
}

export function ResultsView({
  transport,
  path,
  tab,
  features,
  live,
  session,
  onSession,
  onFocusId,
  onInspect,
  reveal,
}: {
  transport: Transport;
  path: string;
  tab: TabDef;
  features: Features | null;
  live: boolean;
  session: TabSession;
  onSession: (s: Partial<TabSession>) => void;
  onFocusId: (id: number | null) => void;
  /** Enter on a row: bring the inspector up and move focus into it. */
  onInspect?: () => void;
  /** A URL to select and scroll to ("show in table"); a new `n` asks again. */
  reveal?: { id: number; n: number } | null;
}) {
  const grid = useRef<GridHandle>(null);
  const [selection, setSelection] = useState<Selection>(emptySelection);
  const [prefs, setPrefs] = useState<ColumnPrefs>(() => loadPrefs(tab));
  const [notice, setNotice] = useState<{ kind: "info" | "warn" | "bad"; text: string } | null>(null);
  const [searchText, setSearchText] = useState(session.search);
  const search = useDebounced(searchText, 200);
  const available = useCallback((need: Need) => need === "base" || (features?.available.includes(need) ?? false), [features]);

  useEffect(() => {
    setPrefs(loadPrefs(tab));
    setSearchText(session.search);
    setSelection(emptySelection);
    // Only when the tab changes; the session is the tab's own.
  }, [tab]);
  // Typing updates the session; a search cleared from outside (show in
  // table) is not put back.
  useEffect(() => {
    if (search !== session.search) onSession({ search });
    // Only when the typed search settles.
  }, [search]);

  const updatePrefs = (p: ColumnPrefs) => {
    setPrefs(p);
    savePrefs(tab.id, p);
  };

  const query: ViewQuery = useMemo(
    () => ({ tab: tab.id, filter: session.filter, scope: tab.scoped ? session.scope : "all", search: session.search, sort: session.sort }),
    [tab, session.filter, session.scope, session.search, session.sort],
  );

  const selectionRef = useRef(selection);
  selectionRef.current = selection;
  const view = useView(transport, path, query, {
    live,
    anchors: () => {
      const top = grid.current?.topRow() ?? 0;
      const ids = [view.row(top)?.[0], selectionRef.current.focus !== null ? view.row(selectionRef.current.focus)?.[0] : undefined];
      return ids.map((x) => (typeof x === "number" ? x : -1));
    },
    onRefresh: (positions, total) => {
      const top = grid.current?.topRow() ?? 0;
      const [newTop, newFocus] = positions;
      if (newTop != null) grid.current?.shiftBy(newTop - top);
      const s = selectionRef.current;
      const delta = s.focus !== null && newFocus != null ? newFocus - s.focus : newTop != null ? newTop - top : 0;
      setSelection(shift(s, delta, total));
    },
  });
  const counts = useCounts(transport, path, tab.id, tab.scoped ? session.scope : "all", view.version);

  // A new query starts at the top with nothing selected, except the row
  // remembered for this tab, which is found again.
  useEffect(() => {
    setSelection(emptySelection);
  }, [query]);
  useEffect(() => {
    if (!view.ready || view.view === null || session.focusId === null) return;
    let off = false;
    transport.viewLocate(path, view.view, [session.focusId]).then(([i]) => {
      if (off || i === null || i === undefined) return;
      setSelection(single(i));
      grid.current?.reveal(i);
    }, () => {});
    return () => {
      off = true;
    };
    // Once per opened view.
  }, [view.view, view.ready]);

  // "Show in table": select the URL here, first clearing the filter, scope
  // or search when they hide it.
  const revealed = useRef(0);
  useEffect(() => {
    if (!reveal || revealed.current === reveal.n || !view.ready || view.view === null) return;
    let off = false;
    transport.viewLocate(path, view.view, [reveal.id]).then(([i]) => {
      if (off) return;
      if (i != null) {
        revealed.current = reveal.n;
        setSelection(single(i));
        grid.current?.reveal(i);
        grid.current?.focus();
      } else if (session.filter !== "all" || session.search || session.scope !== "all") {
        setSearchText("");
        onSession({ filter: "all", search: "", scope: "all" });
      } else revealed.current = reveal.n;
    }, () => {});
    return () => {
      off = true;
    };
    // Once per request and opened view.
  }, [reveal, view.view, view.ready]);

  // The focused row's id feeds the inspector and is remembered per tab.
  const focusRow = selection.focus !== null ? view.row(selection.focus) : undefined;
  const focusId = typeof focusRow?.[0] === "number" ? focusRow[0] : null;
  useEffect(() => {
    if (selection.focus === null) onFocusId(null);
    else if (focusId !== null) {
      onFocusId(focusId);
      if (focusId !== session.focusId) onSession({ focusId });
    }
  }, [focusId, selection.focus, onFocusId, onSession, session.focusId]);

  const columns: GridColumn[] = useMemo(
    () =>
      prefs.order.flatMap((id) => {
        const index = tab.columns.findIndex((c) => c.id === id);
        return index < 0 ? [] : [{ def: tab.columns[index], index, available: available(tab.columns[index].need) }];
      }),
    [prefs.order, tab, available],
  );

  const exp = useExport(transport);
  const selected = count(selection.ranges);

  const copy = async () => {
    if (view.view === null || selected === 0) return;
    if (selected > COPY_LIMIT) {
      setNotice({ kind: "warn", text: `Copy takes up to ${COPY_LIMIT.toLocaleString()} rows. Export the selection to a CSV file instead.` });
      return;
    }
    try {
      await transport.copyRows(path, view.view, selection.ranges, prefs.order);
      setNotice({ kind: "info", text: `Copied ${selected.toLocaleString()} ${selected === 1 ? "row" : "rows"}.` });
    } catch (e) {
      setNotice({ kind: "bad", text: String(e) });
    }
  };
  useEffect(() => {
    if (!notice || notice.kind === "bad") return;
    const t = setTimeout(() => setNotice(null), 4000);
    return () => clearTimeout(t);
  }, [notice]);

  const startExport = async (onlySelected: boolean) => {
    const dest = await transport.pickCsvPath(`${tab.title.toLowerCase().replace(/\s+/g, "-")}${session.filter === "all" ? "" : `-${session.filter}`}`);
    if (!dest) return;
    await exp.start(path, query, prefs.order, onlySelected ? selection.ranges : null, dest);
  };

  const filterLabel = tab.filters.find((f) => f.id === session.filter)?.label ?? "";
  const totalText = view.ready ? view.total.toLocaleString() : `${view.total.toLocaleString()}+`;

  return (
    <section aria-label={tab.title} className="flex h-full min-w-0 flex-col">
      <header className="flex flex-col gap-2 border-b border-line bg-bg px-3 pt-2.5 pb-2">
        <div className="flex items-center gap-2">
          <h1 className="m-0 truncate text-md font-semibold tracking-tight">{tab.title}</h1>
          <span className="text-sm text-fg-3 tabular" aria-live="polite" data-testid="row-count">
            {view.loading && view.view === null ? "Loading…" : `${totalText} ${view.total === 1 ? "URL" : "URLs"}`}
            {selected > 1 && ` · ${selected.toLocaleString()} selected`}
          </span>
          <div className="ml-auto flex items-center gap-1.5">
            {tab.scoped && (
              <Segmented<UrlScope>
                size="sm"
                label="Which URLs"
                value={session.scope}
                onChange={(scope) => onSession({ scope })}
                options={[
                  { value: "all", label: "All" },
                  { value: "internal", label: "Internal" },
                  { value: "external", label: "External" },
                ]}
              />
            )}
            <label className="relative flex items-center">
              <Search aria-hidden className="pointer-events-none absolute left-2 size-3.5 text-fg-3" />
              <Input
                type="search"
                aria-label={`Search ${tab.title}`}
                placeholder={tab.search_column ? "Search address or text" : "Search address"}
                value={searchText}
                onChange={(e) => setSearchText(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Escape") setSearchText("");
                  if (e.key === "ArrowDown" || e.key === "Enter") {
                    e.preventDefault();
                    grid.current?.focus();
                    if (selection.focus === null && view.total > 0) setSelection(single(0));
                  }
                }}
                className="h-6 w-56 pl-7 text-sm"
              />
            </label>
            <ColumnMenu tab={tab} visible={prefs.order} available={available} onChange={(order) => updatePrefs({ ...prefs, order })} onReset={() => updatePrefs(defaultPrefs(tab))} />
            <Menu
              label="Export"
              trigger={
                <Button size="sm" variant="quiet" disabled={exp.job !== null}>
                  <Download aria-hidden className="size-3.5" />
                  Export
                </Button>
              }
              items={[
                { label: view.ready ? `Export ${view.total.toLocaleString()} rows as CSV…` : "Export rows as CSV…", onSelect: () => void startExport(false), disabled: view.view === null },
                { label: `Export ${selected.toLocaleString()} selected as CSV…`, onSelect: () => void startExport(true), disabled: selected === 0 },
              ]}
            />
          </div>
        </div>
        {tab.id === "ai-search" && <AiOverview transport={transport} path={path} live={live} />}
        <FilterStrip tab={tab} active={session.filter} counts={counts} onSelect={(filter) => onSession({ filter })} />
      </header>

      <div className="min-h-0 flex-1">
        {view.error ? (
          <EmptyState title="Could not read the results">{view.error}</EmptyState>
        ) : (
          <Grid
            ref={grid}
            label={`${tab.title}: ${filterLabel}`}
            columns={columns}
            widths={prefs.widths}
            onWidths={(widths) => updatePrefs({ ...prefs, widths })}
            onReorder={(order) => updatePrefs({ ...prefs, order })}
            total={view.total}
            row={view.row}
            ensure={view.ensure}
            selection={selection}
            onSelection={setSelection}
            sort={session.sort}
            onSort={(sort) => onSession({ sort })}
            onCopy={() => void copy()}
            onOpen={onInspect}
            empty={
              !view.loading && (
                <EmptyState icon={<SearchX className="size-6" />} title={session.search ? "No URLs match the search" : `No ${filterLabel === "All" ? "" : `${filterLabel} `}URLs`}>
                  {session.search ? "Try fewer words, or another filter." : live ? "They appear here as the crawl finds them." : null}
                </EmptyState>
              )
            }
          />
        )}
      </div>

      {(notice || exp.job !== null || exp.result) && (
        <footer className="flex h-8 shrink-0 items-center gap-3 border-t border-line bg-chrome px-3 text-sm">
          {exp.job !== null && exp.progress && (
            <>
              <span className="text-fg-2 tabular">
                Exporting {exp.progress.rows_written.toLocaleString()} of {exp.progress.total.toLocaleString()} rows
              </span>
              <Bar value={exp.progress.total ? exp.progress.rows_written / exp.progress.total : 0} tone="accent" label="Export progress" className="w-40" />
              <Button size="sm" variant="quiet" onClick={exp.cancel}>
                Cancel
              </Button>
            </>
          )}
          {exp.job === null && exp.result && (
            <Notice kind={exp.result.state === "failed" ? "bad" : "info"} role="status">
              {exp.result.state === "done"
                ? `Exported ${exp.result.rows_written.toLocaleString()} rows to ${exp.result.path}`
                : exp.result.state === "cancelled"
                  ? "Export cancelled; no file was written."
                  : `Export failed: ${exp.result.error}`}
            </Notice>
          )}
          {notice && (
            <Notice kind={notice.kind} role={notice.kind === "bad" ? "alert" : "status"} className="ml-auto">
              {notice.text}
            </Notice>
          )}
        </footer>
      )}
    </section>
  );
}
