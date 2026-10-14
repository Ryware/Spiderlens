import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
// The results table (plan D6, D7). TanStack Table holds the column model
// (order, widths, resizing); rows are windowed here, since only the rows on
// screen exist in the page and the engine holds the rest.
import { forwardRef, useCallback, useEffect, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState } from "react";
import { getCoreRowModel, useReactTable } from "@tanstack/react-table";
import { ArrowDown, ArrowUp, CircleDashed } from "lucide-react";
import { cn, Tooltip } from "../components/ui";
import { matches } from "../shell/platform";
import { cellTone, formatCell, needReason, numeric, toneText } from "./format";
import { all, contains, extend, single, toggle } from "./selection";
import { frame, scrollFor, scrollToRow, virtualTop } from "./windowing";
export const ROW_PX = 28;
const HEADER_PX = 28;
const MIN_COL = 48;
const EMPTY = [];
export const Grid = forwardRef(function Grid({ label, columns, widths, onWidths, total, row, ensure, selection, onSelection, sort, onSort, onCopy, onReorder, empty }, ref) {
    const scroller = useRef(null);
    const [scrollTop, setScrollTop] = useState(0);
    const [viewport, setViewport] = useState(600);
    const dragging = useRef(null);
    useLayoutEffect(() => {
        const el = scroller.current;
        if (!el)
            return;
        const measure = () => setViewport(Math.max(ROW_PX, el.clientHeight - HEADER_PX));
        measure();
        if (typeof ResizeObserver === "undefined")
            return;
        const ro = new ResizeObserver(measure);
        ro.observe(el);
        return () => ro.disconnect();
    }, []);
    const f = frame(scrollTop, total, ROW_PX, viewport);
    useEffect(() => ensure(f.first, f.last), [ensure, f.first, f.last]);
    const setScroll = useCallback((top) => {
        const el = scroller.current;
        if (!el)
            return;
        el.scrollTop = top;
        setScrollTop(el.scrollTop);
    }, []);
    useImperativeHandle(ref, () => ({
        topRow: () => Math.ceil(virtualTop(scroller.current?.scrollTop ?? 0, total, ROW_PX, viewport) / ROW_PX),
        shiftBy: (rows) => {
            if (rows === 0)
                return;
            const top = virtualTop(scroller.current?.scrollTop ?? 0, total, ROW_PX, viewport);
            setScroll(scrollFor(top + rows * ROW_PX, total, ROW_PX, viewport));
        },
        reveal: (i) => {
            const s = scrollToRow(i, scroller.current?.scrollTop ?? 0, total, ROW_PX, viewport);
            if (s !== null)
                setScroll(s);
        },
        focus: () => scroller.current?.focus(),
    }), [total, viewport, setScroll]);
    const tableColumns = useMemo(() => columns.map((c) => ({ id: c.def.id, header: c.def.label, size: c.def.width, minSize: MIN_COL, maxSize: 2000 })), [columns]);
    const sizing = widths;
    const table = useReactTable({
        data: EMPTY,
        columns: tableColumns,
        state: { columnSizing: sizing },
        onColumnSizingChange: (u) => onWidths(typeof u === "function" ? u(sizing) : u),
        columnResizeMode: "onChange",
        getCoreRowModel: getCoreRowModel(),
    });
    const leaf = table.getVisibleLeafColumns();
    const width = leaf.reduce((n, c) => n + c.getSize(), 0);
    const byId = useMemo(() => new Map(columns.map((c) => [c.def.id, c])), [columns]);
    const moveTo = (i, grow) => {
        if (total === 0)
            return;
        const j = Math.max(0, Math.min(total - 1, i));
        onSelection(grow ? extend(selection, j) : single(j));
        const s = scrollToRow(j, scroller.current?.scrollTop ?? 0, total, ROW_PX, viewport);
        if (s !== null)
            setScroll(s);
    };
    const onKeyDown = (e) => {
        const page = Math.max(1, Math.floor(viewport / ROW_PX) - 1);
        const at = selection.focus ?? -1;
        const keys = {
            ArrowDown: () => moveTo(at + 1, e.shiftKey),
            ArrowUp: () => moveTo(at < 0 ? 0 : at - 1, e.shiftKey),
            PageDown: () => moveTo(at + page, e.shiftKey),
            PageUp: () => moveTo(at - page, e.shiftKey),
            Home: () => moveTo(0, e.shiftKey),
            End: () => moveTo(total - 1, e.shiftKey),
        };
        if (matches(e, "Mod+A")) {
            e.preventDefault();
            onSelection(all(total));
        }
        else if (matches(e, "Mod+C")) {
            e.preventDefault();
            onCopy();
        }
        else if ((matches(e, "Mod+ArrowDown") || matches(e, "Mod+Shift+ArrowDown"))) {
            e.preventDefault();
            moveTo(total - 1, e.shiftKey);
        }
        else if ((matches(e, "Mod+ArrowUp") || matches(e, "Mod+Shift+ArrowUp"))) {
            e.preventDefault();
            moveTo(0, e.shiftKey);
        }
        else if (keys[e.key] && !e.metaKey && !e.ctrlKey && !e.altKey) {
            e.preventDefault();
            keys[e.key]();
        }
    };
    const onRowClick = (e, i) => {
        if (e.shiftKey)
            onSelection(extend(selection, i));
        else if (e.metaKey || e.ctrlKey)
            onSelection(toggle(selection, i));
        else
            onSelection(single(i));
    };
    const rows = [];
    for (let i = f.first; i < f.last; i++) {
        const r = row(i);
        const selected = contains(selection.ranges, i);
        const focused = selection.focus === i;
        rows.push(_jsx("div", { id: `row-${i}`, role: "row", "aria-rowindex": i + 2, "aria-selected": selected, "data-id": r?.[0], onMouseDown: (e) => {
                if (e.shiftKey)
                    e.preventDefault();
            }, onClick: (e) => onRowClick(e, i), className: cn("absolute left-0 flex h-row items-center border-b border-line text-base", selected ? "bg-selection" : i % 2 === 1 ? "bg-surface-2/40" : "bg-surface", focused && "before:absolute before:inset-y-0 before:left-0 before:w-0.5 before:bg-accent"), style: { top: HEADER_PX + f.firstTopPx + (i - f.first) * ROW_PX, width: Math.max(width, 0), minWidth: "100%" }, children: leaf.map((c) => {
                const col = byId.get(c.id);
                const v = r ? (r[col.index + 1] ?? null) : null;
                const text = r ? formatCell(col.def.kind, v) : "";
                const tone = cellTone(col.def.kind, v);
                return (_jsx("div", { role: "gridcell", className: cn("shrink-0 truncate px-2", numeric(col.def.kind) && "text-right tabular", tone ? toneText[tone] : col.def.kind === "url" ? "text-fg" : "text-fg-2", !r && "text-fg-3"), style: { width: c.getSize() }, title: text.length > 40 ? text : undefined, children: r ? text : i === f.first || i % 10 === 0 ? "…" : "" }, c.id));
            }) }, i));
    }
    return (_jsxs("div", { ref: scroller, role: "grid", "aria-label": label, "aria-rowcount": total + 1, "aria-colcount": leaf.length, "aria-multiselectable": true, "aria-activedescendant": selection.focus !== null && selection.focus >= f.first && selection.focus < f.last ? `row-${selection.focus}` : undefined, tabIndex: 0, onKeyDown: onKeyDown, onScroll: (e) => setScrollTop(e.currentTarget.scrollTop), className: "relative h-full overflow-auto bg-surface outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-inset", children: [_jsxs("div", { style: { height: HEADER_PX + f.contentPx, width: Math.max(width, 0), minWidth: "100%" }, className: "relative", children: [_jsx("div", { role: "row", "aria-rowindex": 1, className: "sticky top-0 z-10 flex h-row border-b border-line-strong bg-chrome", style: { width: Math.max(width, 0), minWidth: "100%" }, children: table.getHeaderGroups()[0]?.headers.map((h) => {
                            const col = byId.get(h.id);
                            const on = sort?.column === h.id;
                            const head = (_jsxs("div", { role: "columnheader", "aria-sort": on ? (sort.desc ? "descending" : "ascending") : "none", className: "group relative flex shrink-0 items-center", style: { width: h.getSize() }, children: [_jsxs("button", { type: "button", tabIndex: -1, draggable: true, onDragStart: (e) => {
                                            dragging.current = h.id;
                                            e.dataTransfer.effectAllowed = "move";
                                        }, onDragOver: (e) => dragging.current && dragging.current !== h.id && e.preventDefault(), onDrop: (e) => {
                                            e.preventDefault();
                                            const from = dragging.current;
                                            dragging.current = null;
                                            if (!from || from === h.id)
                                                return;
                                            const ids = leaf.map((c) => c.id).filter((x) => x !== from);
                                            ids.splice(ids.indexOf(h.id) + (leaf.findIndex((c) => c.id === from) < leaf.findIndex((c) => c.id === h.id) ? 1 : 0), 0, from);
                                            onReorder(ids);
                                        }, onClick: () => onSort(!on ? { column: h.id, desc: false } : !sort.desc ? { column: h.id, desc: true } : null), className: cn("flex h-full min-w-0 flex-1 items-center gap-1 px-2 text-sm font-medium hover:text-fg", numeric(col.def.kind) && "flex-row-reverse text-right", on ? "text-fg" : col.available ? "text-fg-2" : "text-fg-3"), children: [!col.available && _jsx(CircleDashed, { "aria-label": "Not collected", className: "size-3 shrink-0" }), _jsx("span", { className: "truncate", children: col.def.label }), on && (sort.desc ? _jsx(ArrowDown, { "aria-hidden": true, className: "size-3 shrink-0 text-accent-text" }) : _jsx(ArrowUp, { "aria-hidden": true, className: "size-3 shrink-0 text-accent-text" }))] }), _jsx("div", { "aria-hidden": true, onPointerDown: h.getResizeHandler(), onDoubleClick: () => onWidths({ ...widths, [h.id]: col.def.width }), className: cn("absolute inset-y-1 -right-1 z-10 w-2 cursor-col-resize after:absolute after:inset-y-0 after:left-1 after:w-px after:bg-line group-hover:after:bg-line-strong", h.column.getIsResizing() && "after:bg-accent") })] }, h.id));
                            return col.available ? head : (_jsx(Tooltip, { content: needReason[col.def.need], children: head }, h.id));
                        }) }), rows] }), total === 0 && empty && _jsx("div", { className: "absolute inset-x-0 top-7 bottom-0", children: empty })] }));
});
