import { jsx as _jsx, jsxs as _jsxs, Fragment as _Fragment } from "react/jsx-runtime";
// One results tab (US1-US4): filters with counts, scope, search, columns,
// the table, copy and export.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Download, Search, SearchX } from "lucide-react";
import { Bar, Button, EmptyState, Input, Menu, Notice, Segmented } from "../components/ui";
import { ColumnMenu } from "./ColumnMenu";
import { defaultPrefs, loadPrefs, savePrefs } from "./columnPrefs";
import { FilterStrip } from "./FilterStrip";
import { Grid } from "./Grid";
import { count, emptySelection, shift, single } from "./selection";
import { useExport } from "./useExport";
import { useView } from "./useView";
/** Most rows copied through the clipboard (spec Assumptions); export does more. */
export const COPY_LIMIT = 10_000;
function useDebounced(value, ms) {
    const [v, setV] = useState(value);
    useEffect(() => {
        const t = setTimeout(() => setV(value), ms);
        return () => clearTimeout(t);
    }, [value, ms]);
    return v;
}
/** Filter counts, refreshed as the view changes but at most every 2 s. */
function useCounts(transport, path, tab, scope, version) {
    const [counts, setCounts] = useState(null);
    const last = useRef(0);
    const busy = useRef(false);
    const [again, setAgain] = useState(0);
    useEffect(() => {
        setCounts(null);
        last.current = 0;
    }, [path, tab, scope]);
    useEffect(() => {
        if (busy.current)
            return;
        const wait = last.current + 2000 - Date.now();
        if (wait > 0) {
            const t = setTimeout(() => setAgain((n) => n + 1), wait);
            return () => clearTimeout(t);
        }
        busy.current = true;
        last.current = Date.now();
        transport
            .viewCounts(path, tab, scope)
            .then(setCounts, () => { })
            .finally(() => (busy.current = false));
    }, [transport, path, tab, scope, version, again]);
    return counts;
}
export function ResultsView({ transport, path, tab, features, live, session, onSession, onFocusId, }) {
    const grid = useRef(null);
    const [selection, setSelection] = useState(emptySelection);
    const [prefs, setPrefs] = useState(() => loadPrefs(tab));
    const [notice, setNotice] = useState(null);
    const [searchText, setSearchText] = useState(session.search);
    const search = useDebounced(searchText, 200);
    const available = useCallback((need) => need === "base" || (features?.available.includes(need) ?? false), [features]);
    useEffect(() => {
        setPrefs(loadPrefs(tab));
        setSearchText(session.search);
        setSelection(emptySelection);
        // Only when the tab changes; the session is the tab's own.
    }, [tab]);
    useEffect(() => {
        if (search !== session.search)
            onSession({ search });
    }, [search, session.search, onSession]);
    const updatePrefs = (p) => {
        setPrefs(p);
        savePrefs(tab.id, p);
    };
    const query = useMemo(() => ({ tab: tab.id, filter: session.filter, scope: tab.scoped ? session.scope : "all", search: session.search, sort: session.sort }), [tab, session.filter, session.scope, session.search, session.sort]);
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
            if (newTop != null)
                grid.current?.shiftBy(newTop - top);
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
        if (!view.ready || view.view === null || session.focusId === null)
            return;
        let off = false;
        transport.viewLocate(path, view.view, [session.focusId]).then(([i]) => {
            if (off || i === null || i === undefined)
                return;
            setSelection(single(i));
            grid.current?.reveal(i);
        }, () => { });
        return () => {
            off = true;
        };
        // Once per opened view.
    }, [view.view, view.ready]);
    // The focused row's id feeds the inspector and is remembered per tab.
    const focusRow = selection.focus !== null ? view.row(selection.focus) : undefined;
    const focusId = typeof focusRow?.[0] === "number" ? focusRow[0] : null;
    useEffect(() => {
        if (selection.focus === null)
            onFocusId(null);
        else if (focusId !== null) {
            onFocusId(focusId);
            if (focusId !== session.focusId)
                onSession({ focusId });
        }
    }, [focusId, selection.focus, onFocusId, onSession, session.focusId]);
    const columns = useMemo(() => prefs.order.flatMap((id) => {
        const index = tab.columns.findIndex((c) => c.id === id);
        return index < 0 ? [] : [{ def: tab.columns[index], index, available: available(tab.columns[index].need) }];
    }), [prefs.order, tab, available]);
    const exp = useExport(transport);
    const selected = count(selection.ranges);
    const copy = async () => {
        if (view.view === null || selected === 0)
            return;
        if (selected > COPY_LIMIT) {
            setNotice({ kind: "warn", text: `Copy takes up to ${COPY_LIMIT.toLocaleString()} rows. Export the selection to a CSV file instead.` });
            return;
        }
        try {
            await transport.copyRows(path, view.view, selection.ranges, prefs.order);
            setNotice({ kind: "info", text: `Copied ${selected.toLocaleString()} ${selected === 1 ? "row" : "rows"}.` });
        }
        catch (e) {
            setNotice({ kind: "bad", text: String(e) });
        }
    };
    useEffect(() => {
        if (!notice || notice.kind === "bad")
            return;
        const t = setTimeout(() => setNotice(null), 4000);
        return () => clearTimeout(t);
    }, [notice]);
    const startExport = async (onlySelected) => {
        const dest = await transport.pickCsvPath(`${tab.title.toLowerCase().replace(/\s+/g, "-")}${session.filter === "all" ? "" : `-${session.filter}`}`);
        if (!dest)
            return;
        await exp.start(path, query, prefs.order, onlySelected ? selection.ranges : null, dest);
    };
    const filterLabel = tab.filters.find((f) => f.id === session.filter)?.label ?? "";
    const totalText = view.ready ? view.total.toLocaleString() : `${view.total.toLocaleString()}+`;
    return (_jsxs("section", { "aria-label": tab.title, className: "flex h-full min-w-0 flex-col", children: [_jsxs("header", { className: "flex flex-col gap-2 border-b border-line bg-bg px-3 pt-2.5 pb-2", children: [_jsxs("div", { className: "flex items-center gap-2", children: [_jsx("h1", { className: "m-0 truncate text-md font-semibold tracking-tight", children: tab.title }), _jsxs("span", { className: "text-sm text-fg-3 tabular", "aria-live": "polite", "data-testid": "row-count", children: [view.loading && view.view === null ? "Loading…" : `${totalText} ${view.total === 1 ? "URL" : "URLs"}`, selected > 1 && ` · ${selected.toLocaleString()} selected`] }), _jsxs("div", { className: "ml-auto flex items-center gap-1.5", children: [tab.scoped && (_jsx(Segmented, { size: "sm", label: "Which URLs", value: session.scope, onChange: (scope) => onSession({ scope }), options: [
                                            { value: "all", label: "All" },
                                            { value: "internal", label: "Internal" },
                                            { value: "external", label: "External" },
                                        ] })), _jsxs("label", { className: "relative flex items-center", children: [_jsx(Search, { "aria-hidden": true, className: "pointer-events-none absolute left-2 size-3.5 text-fg-3" }), _jsx(Input, { type: "search", "aria-label": `Search ${tab.title}`, placeholder: tab.search_column ? "Search address or text" : "Search address", value: searchText, onChange: (e) => setSearchText(e.target.value), onKeyDown: (e) => {
                                                    if (e.key === "Escape")
                                                        setSearchText("");
                                                    if (e.key === "ArrowDown" || e.key === "Enter") {
                                                        e.preventDefault();
                                                        grid.current?.focus();
                                                        if (selection.focus === null && view.total > 0)
                                                            setSelection(single(0));
                                                    }
                                                }, className: "h-6 w-56 pl-7 text-sm" })] }), _jsx(ColumnMenu, { tab: tab, visible: prefs.order, available: available, onChange: (order) => updatePrefs({ ...prefs, order }), onReset: () => updatePrefs(defaultPrefs(tab)) }), _jsx(Menu, { label: "Export", trigger: _jsxs(Button, { size: "sm", variant: "quiet", disabled: exp.job !== null, children: [_jsx(Download, { "aria-hidden": true, className: "size-3.5" }), "Export"] }), items: [
                                            { label: view.ready ? `Export ${view.total.toLocaleString()} rows as CSV…` : "Export rows as CSV…", onSelect: () => void startExport(false), disabled: view.view === null },
                                            { label: `Export ${selected.toLocaleString()} selected as CSV…`, onSelect: () => void startExport(true), disabled: selected === 0 },
                                        ] })] })] }), _jsx(FilterStrip, { tab: tab, active: session.filter, counts: counts, onSelect: (filter) => onSession({ filter }) })] }), _jsx("div", { className: "min-h-0 flex-1", children: view.error ? (_jsx(EmptyState, { title: "Could not read the results", children: view.error })) : (_jsx(Grid, { ref: grid, label: `${tab.title}: ${filterLabel}`, columns: columns, widths: prefs.widths, onWidths: (widths) => updatePrefs({ ...prefs, widths }), onReorder: (order) => updatePrefs({ ...prefs, order }), total: view.total, row: view.row, ensure: view.ensure, selection: selection, onSelection: setSelection, sort: session.sort, onSort: (sort) => onSession({ sort }), onCopy: () => void copy(), empty: !view.loading && (_jsx(EmptyState, { icon: _jsx(SearchX, { className: "size-6" }), title: session.search ? "No URLs match the search" : `No ${filterLabel === "All" ? "" : `${filterLabel} `}URLs`, children: session.search ? "Try fewer words, or another filter." : live ? "They appear here as the crawl finds them." : null })) })) }), (notice || exp.job !== null || exp.result) && (_jsxs("footer", { className: "flex h-8 shrink-0 items-center gap-3 border-t border-line bg-chrome px-3 text-sm", children: [exp.job !== null && exp.progress && (_jsxs(_Fragment, { children: [_jsxs("span", { className: "text-fg-2 tabular", children: ["Exporting ", exp.progress.rows_written.toLocaleString(), " of ", exp.progress.total.toLocaleString(), " rows"] }), _jsx(Bar, { value: exp.progress.total ? exp.progress.rows_written / exp.progress.total : 0, tone: "accent", label: "Export progress", className: "w-40" }), _jsx(Button, { size: "sm", variant: "quiet", onClick: exp.cancel, children: "Cancel" })] })), exp.job === null && exp.result && (_jsx(Notice, { kind: exp.result.state === "failed" ? "bad" : "info", role: "status", children: exp.result.state === "done"
                            ? `Exported ${exp.result.rows_written.toLocaleString()} rows to ${exp.result.path}`
                            : exp.result.state === "cancelled"
                                ? "Export cancelled; no file was written."
                                : `Export failed: ${exp.result.error}` })), notice && (_jsx(Notice, { kind: notice.kind, role: notice.kind === "bad" ? "alert" : "status", className: "ml-auto", children: notice.text }))] }))] }));
}
