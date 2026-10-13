// The results table (plan D6, D7). TanStack Table holds the column model
// (order, widths, resizing); rows are windowed here, since only the rows on
// screen exist in the page and the engine holds the rest.

import { forwardRef, useCallback, useEffect, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import { getCoreRowModel, useReactTable, type ColumnDef as TableColumn, type ColumnSizingState } from "@tanstack/react-table";
import { ArrowDown, ArrowUp, CircleDashed } from "lucide-react";
import type { ColumnDef } from "../generated/ColumnDef";
import type { Sort } from "../generated/Sort";
import { cn, Tooltip } from "../components/ui";
import { matches } from "../shell/platform";
import { cellTone, formatCell, needReason, numeric, toneText } from "./format";
import { all, contains, extend, single, toggle, type Selection } from "./selection";
import type { Row } from "./useView";
import { frame, scrollFor, scrollToRow, virtualTop } from "./windowing";

export const ROW_PX = 28;
const HEADER_PX = 28;
const MIN_COL = 48;

export interface GridColumn {
  def: ColumnDef;
  /** Index of the value in a row, after the id. */
  index: number;
  available: boolean;
}

export interface GridHandle {
  /** First row whose top is in view. */
  topRow(): number;
  /** Scrolls so the content moves with rows that shifted by `rows`. */
  shiftBy(rows: number): void;
  reveal(i: number): void;
  focus(): void;
}

const EMPTY: never[] = [];

export const Grid = forwardRef<
  GridHandle,
  {
    label: string;
    columns: GridColumn[];
    widths: Record<string, number>;
    onWidths: (w: Record<string, number>) => void;
    total: number;
    row: (i: number) => Row | undefined;
    ensure: (first: number, last: number) => void;
    selection: Selection;
    onSelection: (s: Selection) => void;
    sort: Sort | null;
    onSort: (s: Sort | null) => void;
    onCopy: () => void;
    /** New column order after a header is dragged. */
    onReorder: (ids: string[]) => void;
    empty?: React.ReactNode;
  }
>(function Grid({ label, columns, widths, onWidths, total, row, ensure, selection, onSelection, sort, onSort, onCopy, onReorder, empty }, ref) {
  const scroller = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewport, setViewport] = useState(600);
  const dragging = useRef<string | null>(null);

  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const measure = () => setViewport(Math.max(ROW_PX, el.clientHeight - HEADER_PX));
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const f = frame(scrollTop, total, ROW_PX, viewport);
  useEffect(() => ensure(f.first, f.last), [ensure, f.first, f.last]);

  const setScroll = useCallback((top: number) => {
    const el = scroller.current;
    if (!el) return;
    el.scrollTop = top;
    setScrollTop(el.scrollTop);
  }, []);

  useImperativeHandle(
    ref,
    () => ({
      topRow: () => Math.ceil(virtualTop(scroller.current?.scrollTop ?? 0, total, ROW_PX, viewport) / ROW_PX),
      shiftBy: (rows) => {
        if (rows === 0) return;
        const top = virtualTop(scroller.current?.scrollTop ?? 0, total, ROW_PX, viewport);
        setScroll(scrollFor(top + rows * ROW_PX, total, ROW_PX, viewport));
      },
      reveal: (i) => {
        const s = scrollToRow(i, scroller.current?.scrollTop ?? 0, total, ROW_PX, viewport);
        if (s !== null) setScroll(s);
      },
      focus: () => scroller.current?.focus(),
    }),
    [total, viewport, setScroll],
  );

  const tableColumns = useMemo<TableColumn<Row>[]>(
    () => columns.map((c) => ({ id: c.def.id, header: c.def.label, size: c.def.width, minSize: MIN_COL, maxSize: 2000 })),
    [columns],
  );
  const sizing: ColumnSizingState = widths;
  const table = useReactTable<Row>({
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

  const moveTo = (i: number, grow: boolean) => {
    if (total === 0) return;
    const j = Math.max(0, Math.min(total - 1, i));
    onSelection(grow ? extend(selection, j) : single(j));
    const s = scrollToRow(j, scroller.current?.scrollTop ?? 0, total, ROW_PX, viewport);
    if (s !== null) setScroll(s);
  };

  const onKeyDown = (e: KeyboardEvent) => {
    const page = Math.max(1, Math.floor(viewport / ROW_PX) - 1);
    const at = selection.focus ?? -1;
    const keys: Record<string, () => void> = {
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
    } else if (matches(e, "Mod+C")) {
      e.preventDefault();
      onCopy();
    } else if ((matches(e, "Mod+ArrowDown") || matches(e, "Mod+Shift+ArrowDown"))) {
      e.preventDefault();
      moveTo(total - 1, e.shiftKey);
    } else if ((matches(e, "Mod+ArrowUp") || matches(e, "Mod+Shift+ArrowUp"))) {
      e.preventDefault();
      moveTo(0, e.shiftKey);
    } else if (keys[e.key] && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault();
      keys[e.key]();
    }
  };

  const onRowClick = (e: MouseEvent, i: number) => {
    if (e.shiftKey) onSelection(extend(selection, i));
    else if (e.metaKey || e.ctrlKey) onSelection(toggle(selection, i));
    else onSelection(single(i));
  };

  const rows = [];
  for (let i = f.first; i < f.last; i++) {
    const r = row(i);
    const selected = contains(selection.ranges, i);
    const focused = selection.focus === i;
    rows.push(
      <div
        key={i}
        id={`row-${i}`}
        role="row"
        aria-rowindex={i + 2}
        aria-selected={selected}
        data-id={r?.[0] as number | undefined}
        onMouseDown={(e) => {
          if (e.shiftKey) e.preventDefault();
        }}
        onClick={(e) => onRowClick(e, i)}
        className={cn(
          "absolute left-0 flex h-row items-center border-b border-line text-base",
          selected ? "bg-selection" : i % 2 === 1 ? "bg-surface-2/40" : "bg-surface",
          focused && "before:absolute before:inset-y-0 before:left-0 before:w-0.5 before:bg-accent",
        )}
        style={{ top: HEADER_PX + f.firstTopPx + (i - f.first) * ROW_PX, width: Math.max(width, 0), minWidth: "100%" }}
      >
        {leaf.map((c) => {
          const col = byId.get(c.id)!;
          const v = r ? (r[col.index + 1] ?? null) : null;
          const text = r ? formatCell(col.def.kind, v) : "";
          const tone = cellTone(col.def.kind, v);
          return (
            <div
              key={c.id}
              role="gridcell"
              className={cn(
                "shrink-0 truncate px-2",
                numeric(col.def.kind) && "text-right tabular",
                tone ? toneText[tone] : col.def.kind === "url" ? "text-fg" : "text-fg-2",
                !r && "text-fg-3",
              )}
              style={{ width: c.getSize() }}
              title={text.length > 40 ? text : undefined}
            >
              {r ? text : i === f.first || i % 10 === 0 ? "…" : ""}
            </div>
          );
        })}
      </div>,
    );
  }

  return (
    <div
      ref={scroller}
      role="grid"
      aria-label={label}
      aria-rowcount={total + 1}
      aria-colcount={leaf.length}
      aria-multiselectable
      aria-activedescendant={selection.focus !== null && selection.focus >= f.first && selection.focus < f.last ? `row-${selection.focus}` : undefined}
      tabIndex={0}
      onKeyDown={onKeyDown}
      onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
      className="relative h-full overflow-auto bg-surface outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-inset"
    >
      <div style={{ height: HEADER_PX + f.contentPx, width: Math.max(width, 0), minWidth: "100%" }} className="relative">
        <div role="row" aria-rowindex={1} className="sticky top-0 z-10 flex h-row border-b border-line-strong bg-chrome" style={{ width: Math.max(width, 0), minWidth: "100%" }}>
          {table.getHeaderGroups()[0]?.headers.map((h) => {
            const col = byId.get(h.id)!;
            const on = sort?.column === h.id;
            const head = (
              <div
                key={h.id}
                role="columnheader"
                aria-sort={on ? (sort!.desc ? "descending" : "ascending") : "none"}
                className="group relative flex shrink-0 items-center"
                style={{ width: h.getSize() }}
              >
                <button
                  type="button"
                  tabIndex={-1}
                  draggable
                  onDragStart={(e) => {
                    dragging.current = h.id;
                    e.dataTransfer.effectAllowed = "move";
                  }}
                  onDragOver={(e) => dragging.current && dragging.current !== h.id && e.preventDefault()}
                  onDrop={(e) => {
                    e.preventDefault();
                    const from = dragging.current;
                    dragging.current = null;
                    if (!from || from === h.id) return;
                    const ids = leaf.map((c) => c.id).filter((x) => x !== from);
                    ids.splice(ids.indexOf(h.id) + (leaf.findIndex((c) => c.id === from) < leaf.findIndex((c) => c.id === h.id) ? 1 : 0), 0, from);
                    onReorder(ids);
                  }}
                  onClick={() => onSort(!on ? { column: h.id, desc: false } : !sort!.desc ? { column: h.id, desc: true } : null)}
                  className={cn(
                    "flex h-full min-w-0 flex-1 items-center gap-1 px-2 text-sm font-medium hover:text-fg",
                    numeric(col.def.kind) && "flex-row-reverse text-right",
                    on ? "text-fg" : col.available ? "text-fg-2" : "text-fg-3",
                  )}
                >
                  {!col.available && <CircleDashed aria-label="Not collected" className="size-3 shrink-0" />}
                  <span className="truncate">{col.def.label}</span>
                  {on && (sort!.desc ? <ArrowDown aria-hidden className="size-3 shrink-0 text-accent-text" /> : <ArrowUp aria-hidden className="size-3 shrink-0 text-accent-text" />)}
                </button>
                <div
                  aria-hidden
                  onPointerDown={h.getResizeHandler()}
                  onDoubleClick={() => onWidths({ ...widths, [h.id]: col.def.width })}
                  className={cn("absolute inset-y-1 -right-1 z-10 w-2 cursor-col-resize after:absolute after:inset-y-0 after:left-1 after:w-px after:bg-line group-hover:after:bg-line-strong", h.column.getIsResizing() && "after:bg-accent")}
                />
              </div>
            );
            return col.available ? head : (
              <Tooltip key={h.id} content={needReason[col.def.need]}>
                {head}
              </Tooltip>
            );
          })}
        </div>
        {rows}
      </div>
      {total === 0 && empty && <div className="absolute inset-x-0 top-7 bottom-0">{empty}</div>}
    </div>
  );
});
