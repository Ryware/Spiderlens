// In-memory results for UI tests: a few tabs over a list of URLs, with
// filters, search and sort done in JS. The real queries are tested in Rust
// (crates/results/tests).

import type { ColumnDef } from "../generated/ColumnDef";
import type { Comparison } from "../generated/Comparison";
import type { NearDupStatus } from "../generated/NearDupStatus";
import type { Need } from "../generated/Need";
import type { TabDef } from "../generated/TabDef";
import type { UrlDetail } from "../generated/UrlDetail";
import type { AnalysisFinished, AnalysisProgress, ExportProgress, Range, Transport, ViewReady } from "../transport";

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
    ],
    search_column: null,
    scoped: true,
  },
  {
    id: "content",
    title: "Content",
    columns: [
      col("address", "Address", "url", 420),
      col("main_words", "Main content words", "int", 130, "near_dup"),
      col("closest_sim", "Closest similarity", "int", 130, "near_dup"),
      col("closest_url", "Closest match", "url", 320, "near_dup"),
      col("near_count", "Near duplicates", "int", 120, "near_dup"),
      col("cluster", "Cluster", "int", 80, "near_dup"),
    ],
    filters: [
      { id: "all", label: "All", need: "base" },
      { id: "near_duplicates", label: "Near duplicates", need: "near_dup" },
    ],
    search_column: "closest_url",
    scoped: false,
  },
];

/** Fake clusters by URL id: `[ids, similarity to the first]`. Ids 2-5 and 12-13. */
export const fakeClusters: [number[], number[]][] = [
  [
    [2, 3, 4, 5],
    [0, 97, 93, 91],
  ],
  [
    [12, 13],
    [0, 100],
  ],
];

const clusterOf = (id: number) => fakeClusters.findIndex(([ids]) => ids.includes(id));
const nd = (id: number) => {
  const c = clusterOf(id);
  if (c < 0) return null;
  const [ids, sims] = fakeClusters[c];
  const i = ids.indexOf(id);
  const others = ids.map((o, j) => ({ o, sim: i === 0 ? sims[j] : j === 0 ? sims[i] : Math.min(sims[i], sims[j]) })).filter((x) => x.o !== id);
  others.sort((a, b) => b.sim - a.sim);
  return { cluster: c + 1, others };
};

const cell = (u: FakeUrl, c: string, urls: FakeUrl[] = []) => {
  const d = nd(u.id);
  const closest = d ? urls.find((x) => x.id === d.others[0].o) : undefined;
  return (
    {
      address: u.url,
      status_code: u.status,
      title: u.title,
      h1: u.h1 ?? null,
      depth: u.depth,
      main_words: u.title === null ? null : 240,
      closest_sim: d ? d.others[0].sim : null,
      closest_url: closest?.url ?? null,
      near_count: d ? d.others.length : 0,
      cluster: d?.cluster ?? null,
    } as Record<string, string | number | null>
  )[c] ?? null;
};

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
  /** Near duplicate analysis state; "done" once a test or a run says so. */
  nearDup: NearDupStatus["state"];
  /** Whether the file keeps HTML for the comparison view. */
  htmlStored: boolean;
  analysisRuns: number;
  nearDupExports: { kind: string; cluster: number | null; dest: string }[];
  /** Ends a running analysis, as the engine would. */
  finishAnalysis(f?: Partial<AnalysisFinished>): void;
  analysisProgress(p: Partial<AnalysisProgress>): void;
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
  | "copyRows"
  | "exportStart"
  | "exportCancel"
  | "onViewReady"
  | "onExportProgress"
  | "nearDupStatus"
  | "nearDupClusters"
  | "nearDupMembers"
  | "nearDupPairs"
  | "comparePages"
  | "runNearDup"
  | "cancelNearDup"
  | "exportNearDup"
  | "onAnalysisProgress"
  | "onAnalysisFinished"
> & { results: FakeResults } {
  let version = 1;
  let next = 0;
  const views = new Map<number, { tab: TabDef; ids: number[]; version: number }>();
  const readyCbs = new Set<(r: ViewReady) => void>();
  const progressCbs = new Set<(p: ExportProgress) => void>();
  const analysisProgressCbs = new Set<(p: AnalysisProgress) => void>();
  const analysisFinishedCbs = new Set<(f: AnalysisFinished) => void>();
  const results: FakeResults = {
    urls: [...initial],
    copied: [],
    exports: [],
    opened: [],
    tabCountCalls: 0,
    features: ["base"],
    ready: () => {
      for (const [view, v] of views) readyCbs.forEach((cb) => cb({ view, total: v.ids.length }));
    },
    progress: (p) => progressCbs.forEach((cb) => cb({ job: 1, rows_written: 0, total: 0, state: "running", error: null, path: "/out.csv", ...p })),
    add: (urls) => {
      results.urls.push(...urls);
      version++;
    },
    nearDup: "not_run",
    htmlStored: true,
    analysisRuns: 0,
    nearDupExports: [],
    finishAnalysis: (f) => {
      results.nearDup = f?.error || f?.cancelled ? "not_run" : "done";
      if (results.nearDup === "done" && !results.features.includes("near_dup")) results.features.push("near_dup");
      version++;
      analysisFinishedCbs.forEach((cb) =>
        cb({ path: "/crawls/example.com.spiderlens", summary: f?.error || f?.cancelled ? null : { compared: 27, too_short: 0, not_indexable: 0, no_signature: 0, clusters: 2, ms: 40 }, cancelled: false, error: null, ...f }),
      );
    },
    analysisProgress: (p) => analysisProgressCbs.forEach((cb) => cb({ path: "/crawls/example.com.spiderlens", done: 1, total: 3, step: "Comparing pages", ...p })),
  };
  const queries = new Map<number, Parameters<Transport["viewOpen"]>[1]>();
  const select = (q: Parameters<Transport["viewOpen"]>[1]) => {
    const tab = fakeTabs.find((t) => t.id === q.tab)!;
    let us = results.urls.filter((u) => (q.scope === "all" ? true : (q.scope === "internal") === u.internal));
    if (q.filter === "html") us = us.filter((u) => u.title !== null);
    if (q.filter === "missing_h1") us = us.filter((u) => u.title !== null && !u.h1);
    if (q.filter === "client_error") us = us.filter((u) => u.status !== null && u.status >= 400 && u.status < 500);
    if (q.tab === "content") us = us.filter((u) => u.internal && u.status === 200);
    if (q.filter === "near_duplicates") us = us.filter((u) => clusterOf(u.id) >= 0);
    const s = q.search.toLowerCase();
    if (s) us = us.filter((u) => u.url.toLowerCase().includes(s) || (u.title ?? "").toLowerCase().includes(s));
    if (q.sort) {
      const k = q.sort.column;
      const d = q.sort.desc ? -1 : 1;
      us = [...us].sort((a, b) => {
        const x = cell(a, k, results.urls);
        const y = cell(b, k, results.urls);
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
    return [id, ...tab.columns.map((c) => (results.features.includes(c.need) ? cell(u, c.id, results.urls) : null))];
  };
  const page = (view: number, from: number, count: number) => {
    const v = views.get(view)!;
    return { view, version: v.version, from, rows: v.ids.slice(from, from + count).map((id) => row(v.tab, id)), total: v.ids.length, ready: true };
  };
  const address = (id: number) => results.urls.find((u) => u.id === id)?.url ?? `https://example.com/${id}`;
  const paragraphs = (id: number, changed: boolean) => [
    { spans: [{ text: "Hand made leather shoes, stitched in our workshop.", op: "equal" as const }], pair: 0 },
    {
      spans: [
        { text: "Available in ", op: "equal" as const },
        { text: changed ? `colour ${id}` : "black", op: changed ? ("changed" as const) : ("equal" as const) },
        { text: " with free returns.", op: "equal" as const },
      ],
      pair: 1,
    },
  ];
  return {
    results,
    nearDupStatus: async () => ({
      state: results.nearDup,
      reason: results.nearDup === "not_run" ? "Not analysed yet." : null,
      threshold: results.nearDup === "not_run" ? null : 90,
      compared: results.nearDup === "done" ? 27 : 0,
      too_short: 0,
      not_indexable: 0,
      no_signature: 0,
      near_duplicates: results.nearDup === "done" ? 6 : 0,
      clusters: results.nearDup === "done" ? fakeClusters.length : 0,
      html_stored: results.htmlStored,
    }),
    nearDupClusters: async (_p, offset, limit) => ({
      total: results.nearDup === "done" ? fakeClusters.length : 0,
      rows: (results.nearDup === "done" ? fakeClusters : []).slice(offset, offset + limit).map(([ids, sims], i) => ({
        id: i + 1,
        size: ids.length,
        min_sim: Math.min(...sims.slice(1)),
        max_sim: Math.max(...sims.slice(1)),
        representative_id: ids[0],
        representative: address(ids[0]),
        exact: sims.slice(1).every((x) => x === 100),
      })),
    }),
    nearDupMembers: async (_p, cluster, offset, limit) => {
      const [ids, sims] = fakeClusters[cluster - 1] ?? [[], []];
      return {
        total: ids.length,
        representative_id: ids[0] ?? 0,
        rows: ids.slice(offset, offset + limit).map((id, j) => ({ url_id: id, address: address(id), sim: j + offset === 0 ? null : sims[j + offset], exact: sims[j + offset] === 100, representative: j + offset === 0 })),
      };
    },
    nearDupPairs: async (_p, urlId) => {
      const d = results.nearDup === "done" ? nd(urlId) : null;
      return {
        status: results.nearDup === "done" ? "compared" : null,
        near_count: d?.others.length ?? 0,
        near_count_is_min: false,
        cluster: d?.cluster ?? null,
        threshold: results.nearDup === "done" ? 90 : null,
        rows: (d?.others ?? []).map(({ o, sim }) => ({ other_id: o, address: address(o), sim, exact: sim === 100, cluster: d!.cluster })),
      };
    },
    comparePages: async (_p, a, b, full): Promise<Comparison> => {
      const d = nd(a);
      const sim = d?.others.find((x) => x.o === b)?.sim ?? null;
      if (!results.htmlStored) return { sim, exact: sim === 100, unavailable: "html_not_stored", a: null, b: null };
      const side = (id: number, changed: boolean) => ({
        url_id: id,
        address: address(id),
        source: "main",
        paragraphs: full ? [{ spans: [{ text: "Home · Shop · About", op: "excluded" as const }], pair: null }, ...paragraphs(id, changed)] : paragraphs(id, changed),
      });
      return { sim, exact: sim === 100, unavailable: null, a: side(a, sim !== 100), b: side(b, sim !== 100) };
    },
    runNearDup: async () => {
      results.analysisRuns++;
      results.nearDup = "running";
    },
    cancelNearDup: async () => {
      results.finishAnalysis({ cancelled: true });
    },
    exportNearDup: async (_p, kind, cluster, dest) => {
      results.nearDupExports.push({ kind, cluster, dest });
      return 6;
    },
    onAnalysisProgress: async (cb) => {
      analysisProgressCbs.add(cb);
      return () => void analysisProgressCbs.delete(cb);
    },
    onAnalysisFinished: async (cb) => {
      analysisFinishedCbs.add(cb);
      return () => void analysisFinishedCbs.delete(cb);
    },
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
