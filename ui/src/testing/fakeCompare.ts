// In-memory crawl comparison for UI tests: a fixed summary of two crawls of
// example.com and a few group lists. The real comparison is tested in Rust
// (crates/compare/tests).

import type { CompareSettings } from "../generated/CompareSettings";
import type { CrawlInfo } from "../generated/CrawlInfo";
import type { ElementDelta } from "../generated/ElementDelta";
import type { ElementSetting } from "../generated/ElementSetting";
import type { Group } from "../generated/Group";
import type { GroupColumn } from "../generated/GroupColumn";
import type { GroupRow } from "../generated/GroupRow";
import type { Summary } from "../generated/Summary";
import type { UrlDetail } from "../generated/UrlDetail";
import type { CompareProgress, ExportProgress, ScheduleBaseline, Transport } from "../transport";

export const PREVIOUS = "/crawls/example.com-old.spiderlens";

export interface FakeCompare {
  started: { previous: string; current: string; settings: CompareSettings }[];
  opened: { group: Group; search: string }[];
  exports: { view: number; dest: string }[];
  elementCalls: ElementSetting[][];
  closed: number[];
  cancelled: number[];
  /** Sends a progress event for the latest comparison. */
  progress(p: Partial<CompareProgress>): void;
  /** Ends the latest comparison successfully. */
  finish(): void;
  fail(error: string): void;
  /** Origin of the previous crawl; a different one makes setup suggest a mapping. */
  previousStart: string;
  /** What the schedule baseline lookup answers: null is "not a scheduled run", a string is the error. */
  scheduleBaseline: ScheduleBaseline | string | null;
}

const info = (path: string, start: string, urls: number, complete = true): CrawlInfo => ({
  path,
  start_url: start,
  started_at: "2026-10-01T09:00:00Z",
  app_version: "0.1.0",
  format_version: 3,
  status: complete ? "completed" : "stopped",
  complete,
  url_limit_reached: false,
  snapshot: false,
  urls,
  crawled: urls,
});

const ELEMENTS: ElementDelta[] = [
  { element: "title", label: "Page title", kind: "text", on: true, threshold: null, changed: 12, comparable: true, reason: null },
  { element: "word_count", label: "Word count", kind: "percent", on: true, threshold: 10, changed: 30, comparable: true, reason: null },
  { element: "response_time", label: "Response time", kind: "factor", on: false, threshold: 2, changed: 0, comparable: true, reason: null },
  { element: "h1", label: "H1", kind: "text", on: true, threshold: null, changed: 0, comparable: false, reason: "The previous crawl did not read page content." },
];

function summary(cur: string, prevStart: string, elements: ElementDelta[]): Summary {
  return {
    previous: info(PREVIOUS, prevStart, 120),
    current: info(cur, "https://example.com/", 130),
    config_diffs: [{ field: "limits.max_depth", label: "Maximum depth", previous: "No limit", current: "5" }],
    by_state: [{ state: "Crawled", previous: 120, current: 130 }],
    matched: 110,
    new: 20,
    missing: 10,
    not_reached: 0,
    collisions: 0,
    missing_pages: 8,
    lost_indexable: 3,
    low_overlap: false,
    filters: [
      { tab: "internal", tab_title: "Internal", filter: "all", filter_label: "All", previous: 120, current: 130, new: 20, added: 0, removed: 0, missing: 10, not_reached: 0, comparable: true, reason: null },
      { tab: "response-codes", tab_title: "Response codes", filter: "client_error", filter_label: "Client error (4xx)", previous: 4, current: 9, new: 2, added: 4, removed: 1, missing: 0, not_reached: 0, comparable: true, reason: null },
      { tab: "page-titles", tab_title: "Page titles", filter: "missing", filter_label: "Missing", previous: 3, current: 3, new: 0, added: 0, removed: 0, missing: 0, not_reached: 0, comparable: true, reason: null },
      { tab: "h1", tab_title: "H1", filter: "missing", filter_label: "Missing", previous: null, current: 7, new: 0, added: 0, removed: 0, missing: 0, not_reached: 0, comparable: false, reason: "The previous crawl did not read page content." },
    ],
    elements,
  };
}

const COLUMNS: GroupColumn[] = [
  { id: "address_prev", label: "Address (previous)", side: "previous", kind: "url", sortable: true, width: 320 },
  { id: "address_cur", label: "Address (current)", side: "current", kind: "url", sortable: true, width: 320 },
  { id: "title_prev", label: "Title (previous)", side: "previous", kind: "text", sortable: true, width: 220 },
  { id: "title_cur", label: "Title (current)", side: "current", kind: "text", sortable: true, width: 220 },
];

function rows(group: Group): GroupRow[] {
  const n = group.kind === "urls" && group.which === "missing" ? 10 : group.kind === "urls" ? 20 : 12;
  return Array.from({ length: n }, (_, i) => {
    const prev = group.kind === "urls" && group.which === "new" ? null : i + 1;
    const cur = group.kind === "urls" && group.which === "missing" ? null : 500 + i;
    const path = `/page-${i + 1}`;
    return {
      prev_id: prev,
      cur_id: cur,
      values: [
        prev === null ? null : `https://example.com${path}`,
        cur === null ? null : `https://example.com${path}`,
        prev === null ? null : `Old title ${i + 1}`,
        cur === null ? null : `New title ${i + 1}`,
      ],
    };
  });
}

function detail(id: number, title: string): UrlDetail {
  return {
    id,
    url: `https://example.com/page-${id}`,
    url_readable: `example.com/page-${id}`,
    internal: true,
    final_state: "Crawled",
    status: 200,
    status_text: "OK",
    content_type: "text/html",
    depth: 1,
    error_class: null,
    error_label: null,
    error_message: null,
    error_reason: null,
    attempts: 1,
    manual_retry: false,
    not_crawled_reason: null,
    reason_detail: null,
    robots_rule: null,
    found_on: null,
    discovered_via: "link",
    in_sitemap: false,
    fetched_at: null,
    redirects: [],
    redirect_loop: false,
    redirect_over_limit: false,
    inlinks: 3,
    outlinks: 5,
    images: 0,
    size: 2048,
    response_ms: 120,
    indexability: title.startsWith("Old") ? "Indexable" : "Non-indexable: noindex",
    headers: [],
    headings: 1,
    html_stored: false,
    link_metrics: null,
    cookies: [],
    unavailable: {},
    amp: null,
    pdf: null,
    snippet: null,
    spelling: null,
  };
}

type CompareApi = Pick<
  Transport,
  | "compareSuggest"
  | "compareScheduleBaseline"
  | "compareSetup"
  | "compareTestMapping"
  | "compareStart"
  | "compareCancel"
  | "compareSummary"
  | "compareSetElements"
  | "compareOpenGroup"
  | "compareRows"
  | "compareCloseGroup"
  | "compareDetail"
  | "compareReport"
  | "compareExport"
  | "compareClose"
  | "onCompareProgress"
>;

export function fakeCompareTransport(exportProgress: (p: Partial<ExportProgress>) => void): CompareApi & { compare: FakeCompare } {
  let next = 0;
  let view = 0;
  let current = "";
  let elements = ELEMENTS.map((e) => ({ ...e }));
  const views = new Map<number, GroupRow[]>();
  const listeners = new Set<(p: CompareProgress) => void>();
  const send = (p: Partial<CompareProgress>) =>
    listeners.forEach((cb) => cb({ comparison: next, phase: "previous", done: 0, total: 0, error: null, ...p }));
  const compare: FakeCompare = {
    started: [],
    opened: [],
    exports: [],
    elementCalls: [],
    closed: [],
    cancelled: [],
    progress: send,
    finish: () => send({ phase: "done", done: 1, total: 1 }),
    fail: (error) => send({ phase: "failed", error }),
    previousStart: "https://example.com/",
    scheduleBaseline: null,
  };
  const page = (v: number, from: number, count: number) => {
    const all = views.get(v)!;
    return { view: v, columns: COLUMNS, from, total: all.length, rows: all.slice(from, from + count) };
  };
  return {
    compare,
    compareSuggest: async (cur) => (cur === PREVIOUS ? null : PREVIOUS),
    compareScheduleBaseline: async () => {
      if (typeof compare.scheduleBaseline === "string") throw new Error(compare.scheduleBaseline);
      return compare.scheduleBaseline;
    },
    compareSetup: async (previous, cur) => {
      if (previous === cur) throw new Error("Choose two different crawl files: a crawl compared with itself has no changes.");
      const s = summary(cur, compare.previousStart, elements);
      const prevOrigin = new URL(compare.previousStart).origin;
      return {
        previous: { ...s.previous, path: previous },
        current: s.current,
        config_diffs: s.config_diffs,
        suggested_mapping: prevOrigin === "https://example.com" ? null : { find: `${prevOrigin}/`, replace: "https://example.com/", regex: false },
      };
    },
    compareTestMapping: async (_p, _c, rules, samples) =>
      samples
        .filter((s) => s.trim())
        .map((url) => {
          const rule = rules.find((r) => url.includes(r.find));
          const mapped = rule ? url.replace(rule.find, rule.replace) : url;
          const exists = mapped.startsWith("https://example.com/");
          return { url, mapped, exists_in_current: exists, state: exists ? "Crawled" : null };
        }),
    compareStart: async (previous, cur, settings) => {
      compare.started.push({ previous, current: cur, settings });
      current = cur;
      return ++next;
    },
    compareCancel: async (id) => {
      compare.cancelled.push(id);
      send({ comparison: id, phase: "cancelled" });
    },
    compareSummary: async () => summary(current, compare.previousStart, elements),
    compareSetElements: async (_id, settings) => {
      compare.elementCalls.push(settings);
      elements = ELEMENTS.map((e) => {
        const o = settings.find((s) => s.id === e.element);
        if (!o) return { ...e };
        const threshold = o.threshold ?? e.threshold;
        // A higher threshold reports fewer changes, as the engine does.
        const changed = e.element === "word_count" && threshold !== null ? Math.round(300 / threshold) : e.changed;
        return { ...e, on: o.on, threshold, changed: o.on ? changed : 0 };
      });
      return elements;
    },
    compareOpenGroup: async (_id, group, search, _sort, count) => {
      compare.opened.push({ group, search });
      const v = ++view;
      const s = search.toLowerCase();
      views.set(v, rows(group).filter((r) => !s || r.values.some((x) => typeof x === "string" && x.toLowerCase().includes(s))));
      return page(v, 0, count);
    },
    compareRows: async (_id, v, from, count) => page(v, from, count),
    compareCloseGroup: async (_id, v) => void views.delete(v),
    compareDetail: async (_id, prev, cur) => ({
      previous: prev === null ? null : detail(prev, "Old"),
      current: cur === null ? null : detail(cur, "New"),
    }),
    compareReport: async () => ({ summary: summary(current, compare.previousStart, elements), top_filters: [], elements: [], issues_available: false, mapping: [] }),
    compareExport: async (_id, v, dest) => {
      compare.exports.push({ view: v, dest });
      setTimeout(() => exportProgress({ job: 7, state: "done", rows_written: views.get(v)?.length ?? 0, total: views.get(v)?.length ?? 0, path: dest }));
      return 7;
    },
    compareClose: async (id) => void compare.closed.push(id),
    onCompareProgress: async (cb) => {
      listeners.add(cb);
      return () => void listeners.delete(cb);
    },
  };
}
