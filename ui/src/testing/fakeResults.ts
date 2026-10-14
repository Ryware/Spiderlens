// In-memory results for UI tests: a few tabs over a list of URLs, with
// filters, search and sort done in JS. The real queries are tested in Rust
// (crates/results/tests).

import type { ColumnDef } from "../generated/ColumnDef";
import type { Need } from "../generated/Need";
import type { TabDef } from "../generated/TabDef";
import type { UrlDetail } from "../generated/UrlDetail";
import type { ExportProgress, Range, Transport, ViewReady } from "../transport";

export interface FakeUrl {
  id: number;
  url: string;
  status: number | null;
  title: string | null;
  depth: number;
  internal: boolean;
  /** First H1, when the crawl collected page content. */
  h1?: string | null;
  /** Requests made, retries included (spec 004). */
  attempts?: number;
  /** The result came from "Retry failed URLs". */
  manual_retry?: boolean;
}

const col = (id: string, label: string, kind: ColumnDef["kind"], width: number, need: ColumnDef["need"] = "base", default_visible = true): ColumnDef => ({
  id,
  label,
  kind,
  width,
  default_visible,
  need,
});

export const fakeTabs: TabDef[] = [
  {
    id: "internal",
    title: "Internal",
    columns: [col("address", "Address", "url", 420), col("status_code", "Status code", "status", 96), col("title", "Title", "text", 320), col("h1", "H1", "text", 260, "extraction"), col("depth", "Depth", "int", 70, "base", false)],
    filters: [
      { id: "all", label: "All", need: "base" },
      { id: "html", label: "HTML", need: "base" },
      { id: "missing_h1", label: "Missing H1", need: "extraction" },
    ],
    search_column: "title",
    scoped: false,
  },
  {
    id: "response-codes",
    title: "Response codes",
    columns: [col("address", "Address", "url", 420), col("status_code", "Status code", "status", 96)],
    filters: [
      { id: "all", label: "All", need: "base" },
      { id: "client_error", label: "Client error (4xx)", need: "base" },
      { id: "class_timeout", label: "Timed out", need: "base", hide_when_empty: true },
      { id: "class_http_404", label: "Not found (404)", need: "base", hide_when_empty: true },
    ],
    search_column: null,
    scoped: true,
  },
  {
    id: "links",
    title: "Links",
    columns: [col("address", "Address", "url", 420), col("link_score", "Link Score", "score", 100, "analysis"), col("depth", "Crawl depth", "int", 100), col("followed_inlinks", "Unique followed inlinks", "int", 170, "analysis")],
    filters: [
      { id: "all", label: "All", need: "base" },
      { id: "depth_4", label: "Crawl depth 4 or more", need: "base" },
    ],
    search_column: null,
    scoped: false,
  },
];

/** A made-up Link Score: the start URL is the strongest page. */
const fakeScore = (u: FakeUrl) => (u.id === 1 ? 100 : ((u.id * 37) % 1000) / 10);
const cell = (u: FakeUrl, c: string) =>
  ({ address: u.url, status_code: u.status, title: u.title, h1: u.h1 ?? null, depth: u.depth, link_score: fakeScore(u), followed_inlinks: u.id % 7 })[c] ?? null;

export interface FakeResults {
  urls: FakeUrl[];
  copied: string[];
  exports: { columns: string[]; ranges: Range[] | null; dest: string }[];
  opened: { tab: string; filter: string; search: string; sort: string | null }[];
  /** How often the sidebar asked for its tab counts. */
  tabCountCalls: number;
  /** What the open crawl file holds; base only unless a test adds more. */
  features: Need[];
  /** Announces that the open view's full order is ready. */
  ready(): void;
  progress(p: Partial<ExportProgress>): void;
  /** Adds URLs, as a running crawl would; the next refresh sees them. */
  add(urls: FakeUrl[]): void;
}

export function fakeResultsTransport(initial: FakeUrl[]): Pick<
  Transport,
  | "resultsTabs"
  | "resultsOpen"
  | "viewOpen"
  | "viewRows"
  | "viewLocate"
  | "viewRefresh"
  | "viewCounts"
  | "tabCounts"
  | "viewClose"
  | "urlDetail"
  | "urlList"
  | "urlHtml"
  | "urlTabs"
  | "copyText"
  | "copyRows"
  | "exportStart"
  | "exportCancel"
  | "onViewReady"
  | "onExportProgress"
> & { results: FakeResults } {
  let version = 1;
  let next = 0;
  const views = new Map<number, { tab: TabDef; ids: number[]; version: number }>();
  const readyCbs = new Set<(r: ViewReady) => void>();
  const progressCbs = new Set<(p: ExportProgress) => void>();
  const results: FakeResults = {
    urls: [...initial],
    copied: [],
    exports: [],
    opened: [],
    tabCountCalls: 0,
    features: ["base", "analysis"],
    ready: () => {
      for (const [view, v] of views) readyCbs.forEach((cb) => cb({ view, total: v.ids.length }));
    },
    progress: (p) => progressCbs.forEach((cb) => cb({ job: 1, rows_written: 0, total: 0, state: "running", error: null, path: "/out.csv", ...p })),
    add: (urls) => {
      results.urls.push(...urls);
      version++;
    },
  };
  const queries = new Map<number, Parameters<Transport["viewOpen"]>[1]>();
  const select = (q: Parameters<Transport["viewOpen"]>[1]) => {
    const tab = fakeTabs.find((t) => t.id === q.tab)!;
    let us = results.urls.filter((u) => (q.scope === "all" ? true : (q.scope === "internal") === u.internal));
    if (q.filter === "html") us = us.filter((u) => u.title !== null);
    if (q.filter === "missing_h1") us = us.filter((u) => u.title !== null && !u.h1);
    if (q.filter === "depth_4") us = us.filter((u) => u.depth >= 4);
    if (q.filter === "class_timeout") us = [];
    if (q.filter === "class_http_404") us = us.filter((u) => u.status === 404);
    if (q.filter === "client_error") us = us.filter((u) => u.status !== null && u.status >= 400 && u.status < 500);
    const s = q.search.toLowerCase();
    if (s) us = us.filter((u) => u.url.toLowerCase().includes(s) || (u.title ?? "").toLowerCase().includes(s));
    if (q.sort) {
      const k = q.sort.column;
      const d = q.sort.desc ? -1 : 1;
      us = [...us].sort((a, b) => {
        const x = cell(a, k);
        const y = cell(b, k);
        if (x === y) return a.id - b.id;
        if (x === null) return 1;
        if (y === null) return -1;
        return (x < y ? -1 : 1) * d;
      });
    }
    return { tab, ids: us.map((u) => u.id) };
  };
  const row = (tab: TabDef, id: number) => {
    const u = results.urls.find((x) => x.id === id)!;
    return [id, ...tab.columns.map((c) => (results.features.includes(c.need) ? cell(u, c.id) : null))];
  };
  const page = (view: number, from: number, count: number) => {
    const v = views.get(view)!;
    return { view, version: v.version, from, rows: v.ids.slice(from, from + count).map((id) => row(v.tab, id)), total: v.ids.length, ready: true };
  };
  return {
    results,
    resultsTabs: async () => fakeTabs,
    resultsOpen: async () => ({ features: { available: [...results.features], redirect_limit: 10 } }),
    viewOpen: async (_p, q, count) => {
      const view = ++next;
      results.opened.push({ tab: q.tab, filter: q.filter, search: q.search, sort: q.sort ? `${q.sort.column}${q.sort.desc ? ":desc" : ""}` : null });
      views.set(view, { ...select(q), version });
      queries.set(view, q);
      return { ...page(view, 0, count), total: null, ready: false };
    },
    viewRows: async (_p, view, from, count) => page(view, from, count),
    viewLocate: async (_p, view, ids) => ids.map((id) => {
      const i = views.get(view)!.ids.indexOf(id);
      return i < 0 ? null : i;
    }),
    viewRefresh: async (_p, view, anchors) => {
      const v = views.get(view)!;
      if (v.version === version) return null;
      const fresh = { ...select(queries.get(view)!), version };
      views.set(view, fresh);
      return { version, total: fresh.ids.length, positions: anchors.map((a) => (fresh.ids.indexOf(a) < 0 ? null : fresh.ids.indexOf(a))), build_ms: 1 };
    },
    viewCounts: async (_p, tab, scope) => {
      const t = fakeTabs.find((x) => x.id === tab)!;
      const counts: Record<string, number | null> = {};
      const unavailable: Record<string, string> = {};
      for (const f of t.filters) {
        if (!results.features.includes(f.need)) {
          counts[f.id] = null;
          unavailable[f.id] = "Not collected yet: arrives with page extraction.";
        } else counts[f.id] = select({ tab, filter: f.id, scope, search: "", sort: null }).ids.length;
      }
      return { tab, counts, unavailable, version };
    },
    tabCounts: async () => {
      results.tabCountCalls++;
      return { counts: Object.fromEntries(fakeTabs.map((t) => [t.id, select({ tab: t.id, filter: "all", scope: "all", search: "", sort: null }).ids.length])), version };
    },
    viewClose: async (_p, view) => {
      views.delete(view);
    },
    urlDetail: async (_p, id) => {
      const u = results.urls.find((x) => x.id === id)!;
      const d: UrlDetail = {
        id,
        url: u.url,
        url_readable: u.url,
        internal: u.internal,
        final_state: "Crawled",
        status: u.status,
        status_text: u.status === 200 ? "OK" : null,
        content_type: "text/html",
        depth: u.depth,
        error_class: null,
        error_label: null,
        error_message: null,
        not_crawled_reason: null,
        reason_detail: null,
        robots_rule: null,
        found_on: null,
        discovered_via: "link",
        in_sitemap: false,
        fetched_at: 0,
        redirects: [],
        redirect_loop: false,
        redirect_over_limit: false,
        inlinks: 1,
        outlinks: 0,
        images: 0,
        size: 2048,
        response_ms: 120,
        indexability: "Indexable",
        headers: [
          ["content-type", "text/html"],
          ["x-robots-tag", "noarchive"],
        ],
        headings: 2,
        html_stored: results.features.includes("raw_html") && u.title !== null,
        link_metrics: null,
        attempts: results.features.includes("attempts") ? (u.attempts ?? 1) : null,
        error_reason: null,
        manual_retry: u.manual_retry ?? false,
        unavailable: {
          ...(results.features.includes("attempts") ? {} : { attempts: "Request attempts are not recorded in this crawl yet." }),
          ...(results.features.includes("raw_html") ? {} : { raw_html: "This crawl does not store page HTML." }),
        },
      };
      return d;
    },
    urlList: async (_p, id, kind, from) =>
      kind === "inlinks"
        ? { total: 1, from, rows: [{ id: 1, url: results.urls[0].url, raw: "/", kind: "a", nofollow: false, followed: true, internal: true, status: 200, final_state: "Crawled", text: "Home", sponsored: false, ugc: false }].filter(() => id !== -1), headings: [], unavailable: null }
        : kind === "headings"
          ? { total: 2, from, rows: [], headings: [{ level: 1, text: `Heading for ${id}` }, { level: 2, text: "Section one" }], unavailable: null }
          : kind === "images"
            ? {
                total: 2,
                from,
                rows: [null, ""].map((text, i) => ({ id: 100 + i, url: `https://example.com/img-${i}.png`, raw: `/img-${i}.png`, kind: "image", nofollow: false, followed: true, internal: true, status: 200, final_state: "Crawled", text, sponsored: false, ugc: false })),
                headings: [],
                unavailable: null,
              }
            : { total: 0, from, rows: [], headings: [], unavailable: null },
    urlHtml: async (_p, id) => {
      const u = results.urls.find((x) => x.id === id)!;
      const none = { html: null, bytes: 0, encoding: null, encoding_errors: false };
      if (!results.features.includes("raw_html")) return { ...none, missing: "not_stored" };
      if (u.title === null) return { ...none, missing: "no_body" };
      const html = fakePageHtml(u);
      return { html, missing: null, bytes: new TextEncoder().encode(html).length, encoding: "UTF-8", encoding_errors: false };
    },
    // Internal URLs are in both fake tabs, external ones only in Response
    // codes; ids outside `urls` (the fake list rows) are in no tab.
    urlTabs: async (_p, id) => {
      const u = results.urls.find((x) => x.id === id);
      return !u ? [] : u.internal ? ["internal", "response-codes"] : ["response-codes"];
    },
    copyText: async (text) => {
      results.copied.push(text);
    },
    copyRows: async (_p, view, ranges, columns) => {
      const v = views.get(view)!;
      const cols = columns.map((c) => v.tab.columns.findIndex((x) => x.id === c));
      const lines = [cols.map((i) => v.tab.columns[i].label).join("\t")];
      for (const [a, b] of ranges) for (const id of v.ids.slice(a, b)) lines.push(cols.map((i) => String(row(v.tab, id)[i + 1] ?? "")).join("\t"));
      const text = lines.join("\n");
      results.copied.push(text);
      return text;
    },
    exportStart: async (_p, _q, columns, ranges, dest) => {
      results.exports.push({ columns, ranges, dest });
      return 1;
    },
    exportCancel: async () => {},
    onViewReady: async (cb) => {
      readyCbs.add(cb);
      return () => void readyCbs.delete(cb);
    },
    onExportProgress: async (cb) => {
      progressCbs.add(cb);
      return () => void progressCbs.delete(cb);
    },
  };
}

export function fakeUrls(n: number): FakeUrl[] {
  return Array.from({ length: n }, (_, i) => ({
    id: i + 1,
    url: i === 0 ? "https://example.com/" : `https://example.com/page-${i}`,
    status: i % 10 === 9 ? 404 : 200,
    title: i % 10 === 9 ? null : `Page ${i}`,
    depth: i === 0 ? 0 : 1,
    internal: true,
  }));
}

/** The stored source of a fake page: long enough to need windowing. */
export function fakePageHtml(u: FakeUrl, paragraphs = 2000): string {
  const body = Array.from({ length: paragraphs }, (_, i) => `    <p>Paragraph ${i + 1} of ${u.url}</p>`);
  return ["<!doctype html>", "<html>", "  <head>", `    <title>${u.title}</title>`, "  </head>", "  <body>", ...body, "  </body>", "</html>", ""].join("\n");
}
