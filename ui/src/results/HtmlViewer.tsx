// Source viewer for a page's stored HTML (spec 007 T029, spec 005 FR-030).
// Pages can be megabytes, often on one line, so the source is cut into rows
// of at most WRAP_COLS characters and only the rows on screen are rendered
// (the same windowing as the results table, plan D7).

import { useDeferredValue, useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { ChevronDown, ChevronUp, Copy, FileX } from "lucide-react";
import type { HtmlVersion } from "../generated/HtmlVersion";
import type { PageHtml } from "../generated/PageHtml";
import type { Transport } from "../transport";
import { matches as keyMatches } from "../shell/platform";
import { Button, EmptyState, Input, Notice, Sheet, SheetClose, cn } from "../components/ui";
import { formatBytes } from "./format";
import { frame, scrollToRow } from "./windowing";

export const LINE_PX = 20;
export const WRAP_COLS = 160;
/** Search stops counting here; the count then reads "10,000+". */
export const MAX_MATCHES = 10_000;

/** Display rows: [start, end) offsets into the text, and the 1-based source
 * line a row starts (0 for the rest of a wrapped line). */
export interface Rows {
  start: Int32Array;
  end: Int32Array;
  line: Int32Array;
  lines: number;
}

const isHigh = (c: number) => c >= 0xd800 && c <= 0xdbff;

export function layoutRows(text: string, cols = WRAP_COLS): Rows {
  const start: number[] = [];
  const end: number[] = [];
  const line: number[] = [];
  let n = 0;
  let from = 0;
  for (;;) {
    const nl = text.indexOf("\n", from);
    const stop = nl < 0 ? text.length : nl;
    const le = stop > from && text.charCodeAt(stop - 1) === 13 ? stop - 1 : stop;
    n++;
    let s = from;
    do {
      let e = Math.min(s + cols, le);
      // Never split a surrogate pair across two rows.
      if (e < le && e - s > 1 && isHigh(text.charCodeAt(e - 1))) e--;
      start.push(s);
      end.push(e);
      line.push(s === from ? n : 0);
      s = e;
    } while (s < le);
    // A final line break ends the last line; it does not start another.
    if (nl < 0 || nl + 1 === text.length) break;
    from = nl + 1;
  }
  return { start: Int32Array.from(start), end: Int32Array.from(end), line: Int32Array.from(line), lines: n };
}

export interface Matches {
  start: Int32Array;
  end: Int32Array;
  /** Stopped at MAX_MATCHES. */
  capped: boolean;
}

const NO_MATCHES: Matches = { start: new Int32Array(0), end: new Int32Array(0), capped: false };

/** Every case-insensitive occurrence of `query`, in order. */
export function findMatches(text: string, query: string, max = MAX_MATCHES): Matches {
  if (!query) return NO_MATCHES;
  const re = new RegExp(query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "gi");
  const start: number[] = [];
  const end: number[] = [];
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) {
    if (start.length === max) return { start: Int32Array.from(start), end: Int32Array.from(end), capped: true };
    start.push(m.index);
    end.push(m.index + m[0].length);
  }
  return { start: Int32Array.from(start), end: Int32Array.from(end), capped: false };
}

/** Index of the last element <= x in a sorted array, or -1. */
function lastAtOrBefore(a: Int32Array, x: number): number {
  let lo = 0;
  let hi = a.length - 1;
  let found = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (a[mid] <= x) {
      found = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  return found;
}

/** The text of one row with the matches inside it marked. */
function rowContent(text: string, s: number, e: number, m: Matches, current: number): ReactNode {
  if (m.start.length === 0) return text.slice(s, e);
  const parts: ReactNode[] = [];
  let at = s;
  // Matches do not overlap, so their ends are sorted too.
  let i = Math.max(0, lastAtOrBefore(m.start, s));
  for (; i < m.start.length && m.start[i] < e; i++) {
    if (m.end[i] <= s) continue;
    const a = Math.max(m.start[i], s);
    const b = Math.min(m.end[i], e);
    if (a > at) parts.push(text.slice(at, a));
    parts.push(
      <mark key={i} data-current={i === current || undefined} className={cn("rounded-sm", i === current ? "bg-accent text-accent-fg" : "bg-warn-soft text-fg")}>
        {text.slice(a, b)}
      </mark>,
    );
    at = b;
  }
  if (at < e) parts.push(text.slice(at, e));
  return parts;
}

function SourceView({ text, rows, found, current }: { text: string; rows: Rows; found: Matches; current: number }) {
  const scroller = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewport, setViewport] = useState(600);
  const total = rows.start.length;

  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const measure = () => el.clientHeight > 0 && setViewport(el.clientHeight);
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Bring the current match into view.
  useEffect(() => {
    if (current < 0 || current >= found.start.length) return;
    const row = lastAtOrBefore(rows.start, found.start[current]);
    const top = scrollToRow(row, scrollTop, total, LINE_PX, viewport);
    if (top === null) return;
    if (scroller.current) scroller.current.scrollTop = top;
    setScrollTop(top);
    // Only a new current match scrolls; the user's own scrolling must not.
  }, [current, found, rows]);

  const f = frame(scrollTop, total, LINE_PX, viewport);
  const gutter = `${String(rows.lines).length + 2}ch`;
  const out: ReactNode[] = [];
  for (let i = f.first; i < f.last; i++) {
    const n = rows.line[i];
    out.push(
      <div key={i} className="absolute left-0 flex w-full" style={{ top: f.firstTopPx + (i - f.first) * LINE_PX, height: LINE_PX, lineHeight: `${LINE_PX}px` }} data-row={i}>
        <span aria-hidden className="shrink-0 border-r border-line pr-2 text-right text-fg-3 select-none" style={{ width: gutter }}>
          {n > 0 ? n : ""}
        </span>
        <span className="pl-3 whitespace-pre text-fg" data-selectable>
          {rowContent(text, rows.start[i], rows.end[i], found, current)}
        </span>
      </div>,
    );
  }
  return (
    <div
      ref={scroller}
      role="region"
      aria-label="Page source"
      tabIndex={0}
      className="min-h-0 flex-1 overflow-auto bg-surface font-code text-xs focus-visible:outline-none"
      onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
    >
      <div className="relative min-w-full" style={{ height: f.contentPx, width: `calc(${gutter} + ${WRAP_COLS + 4}ch)` }}>
        {out}
      </div>
    </div>
  );
}

type Load = { state: "loading" } | { state: "error"; message: string } | { state: "done"; page: PageHtml };

export function HtmlViewer({
  transport,
  path,
  id,
  url,
  open,
  onOpenChange,
  version = "raw",
}: {
  transport: Transport;
  path: string;
  id: number;
  url: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The HTML as received (default) or the DOM after scripts ran (spec 014 FR-032). */
  version?: HtmlVersion;
}) {
  const title = version === "rendered" ? "Rendered HTML" : "Page HTML";
  const [load, setLoad] = useState<Load>({ state: "loading" });
  const [query, setQuery] = useState("");
  const [cur, setCur] = useState({ q: "", i: 0 });
  const [copied, setCopied] = useState<"copied" | "failed" | null>(null);
  const search = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    let off = false;
    setLoad({ state: "loading" });
    setQuery("");
    setCopied(null);
    transport.urlHtml(path, id, version).then(
      (page) => !off && setLoad({ state: "done", page }),
      (e) => !off && setLoad({ state: "error", message: String(e) }),
    );
    return () => {
      off = true;
    };
  }, [transport, path, id, open, version]);

  const html = load.state === "done" ? load.page.html : null;
  const rows = useMemo(() => (html === null ? null : layoutRows(html)), [html]);
  const q = useDeferredValue(query);
  const found = useMemo(() => (html === null ? NO_MATCHES : findMatches(html, q)), [html, q]);
  const count = found.start.length;
  const current = count === 0 ? -1 : cur.q === q ? Math.min(cur.i, count - 1) : 0;
  const step = (d: number) => count > 0 && setCur({ q, i: (current + d + count) % count });

  const onSearchKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key !== "Enter") return;
    e.preventDefault();
    step(e.shiftKey ? -1 : 1);
  };
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!keyMatches(e, "Mod+F")) return;
    e.preventDefault();
    e.stopPropagation();
    search.current?.focus();
    search.current?.select();
  };
  const copy = () => {
    if (html === null) return;
    const done = navigator.clipboard?.writeText(html);
    if (!done) return setCopied("failed");
    done.then(
      () => setCopied("copied"),
      () => setCopied("failed"),
    );
  };

  let body: ReactNode;
  if (load.state === "loading") {
    body = (
      <div className="flex flex-1 items-center justify-center text-sm text-fg-3" aria-busy>
        Loading…
      </div>
    );
  } else if (load.state === "error") {
    body = (
      <div className="flex-1 p-4">
        <Notice kind="bad" role="alert">
          {load.message}
        </Notice>
      </div>
    );
  } else if (load.page.missing === "not_stored") {
    body = (
      <EmptyState icon={<FileX className="size-6" />} title={`${title} was not stored`}>
        This crawl ran with <span className="text-fg-2">Store page HTML</span> turned off. Turn it on in Settings › Page data › Store page HTML, then run a new crawl to keep each page's source.
      </EmptyState>
    );
  } else if (html === null || !rows) {
    body = (
      <EmptyState icon={<FileX className="size-6" />} title="No HTML for this URL">
        It is not an HTML page, or no body was received, so there is no source to show.
      </EmptyState>
    );
  } else {
    body = <SourceView text={html} rows={rows} found={found} current={current} />;
  }

  const page = load.state === "done" ? load.page : null;
  return (
    <Sheet open={open} onOpenChange={onOpenChange} title={title} className="h-180 w-240 sheet-max-w" initialFocus={search}>
      <div className="flex min-h-0 flex-1 flex-col" onKeyDown={onKey}>
        <header className="flex flex-col gap-0.5 border-b border-line px-4 pt-4 pb-3">
          <h2 className="m-0 text-md font-semibold">{title}</h2>
          <p className="m-0 truncate text-sm text-fg-3" title={url} data-selectable>
            {url}
          </p>
        </header>
        <div className="flex items-center gap-2 border-b border-line px-4 py-2">
          <Input
            ref={search}
            type="search"
            aria-label="Find in source"
            placeholder="Find in source"
            className="w-64"
            value={query}
            disabled={html === null}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onSearchKey}
          />
          <Button icon variant="quiet" aria-label="Previous match" disabled={count === 0} onClick={() => step(-1)}>
            <ChevronUp className="size-4" />
          </Button>
          <Button icon variant="quiet" aria-label="Next match" disabled={count === 0} onClick={() => step(1)}>
            <ChevronDown className="size-4" />
          </Button>
          <span className="text-sm text-fg-3 tabular" role="status" data-testid="match-count">
            {q && html !== null ? (count === 0 ? "No matches" : `${(current + 1).toLocaleString()} of ${count.toLocaleString()}${found.capped ? "+" : ""}`) : ""}
          </span>
          <div className="ml-auto flex items-center gap-2">
            {copied && (
              <span role="status" className={cn("text-sm", copied === "failed" ? "text-bad" : "text-fg-3")}>
                {copied === "failed" ? "Could not copy." : "Copied."}
              </span>
            )}
            <Button disabled={html === null} onClick={copy}>
              <Copy className="size-3.5" /> Copy
            </Button>
          </div>
        </div>
        {body}
        <footer className="flex items-center gap-2 border-t border-line px-4 py-2">
          <span className="mr-auto text-sm text-fg-3 tabular" data-testid="html-facts">
            {page && rows
              ? [formatBytes(page.bytes), `${rows.lines.toLocaleString()} ${rows.lines === 1 ? "line" : "lines"}`, page.encoding].filter(Boolean).join(" · ")
              : ""}
          </span>
          {page?.encoding_errors && <Notice kind="warn">Some bytes are not valid {page.encoding} and show as �.</Notice>}
          <SheetClose render={<Button variant="primary" />}>Close</SheetClose>
        </footer>
      </div>
    </Sheet>
  );
}
