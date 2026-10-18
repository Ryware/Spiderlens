// In-memory results for UI tests: a few tabs over a list of URLs, with
// filters, search and sort done in JS. The real queries are tested in Rust
// (crates/results/tests).

import type { ColumnDef } from "../generated/ColumnDef";
import type { Comparison } from "../generated/Comparison";
import type { NearDupStatus } from "../generated/NearDupStatus";
import type { CookieFilter } from "../generated/CookieFilter";
import type { CookieRow } from "../generated/CookieRow";
import type { HtmlDiff } from "../generated/HtmlDiff";
import type { Need } from "../generated/Need";
import type { PdfDetail } from "../generated/PdfDetail";
import type { RenderDetail } from "../generated/RenderDetail";
import type { Segment } from "../generated/Segment";
import type { SegmentCounts } from "../generated/SegmentCounts";
import type { TabDef } from "../generated/TabDef";
import type { UrlDetail } from "../generated/UrlDetail";
import type { ViewState } from "../generated/ViewState";
import type { ExportOptions } from "../generated/ExportOptions";
import { fakeScene, fakeTermLinks } from "./fakeViz";
import type { LinkAnalysisSettings } from "../generated/LinkAnalysisSettings";
import type { AnalysisProgress } from "../generated/AnalysisProgress";
import type { AnalysisRun } from "../generated/AnalysisRun";
import type { AnalysisDone, AnalysisProgressEvent, ExportProgress, NearDupFinished, NearDupProgress, Range, Transport, ViewReady } from "../transport";
import { fakeIssueTabs, fakeIssuesTransport, hits, type FakeIssues } from "./fakeIssues";

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
  /** Failed AMP checks, for AMP pages. */
  ampFailures?: number;
  /** What was read, for a PDF (spec 027 part C). */
  pdf?: PdfDetail;
  /** Meta description, for the search preview. */
  description?: string | null;
  /** Spelling errors, once spelling was checked. */
  spellErrors?: number;
}

/** A PDF's detail with sensible defaults. */
export const fakePdf = (p: Partial<PdfDetail> = {}): PdfDetail => ({
  status: "read",
  status_label: "Read",
  status_reason: null,
  title: "Annual report",
  author: "Ada",
  subject: null,
  keywords: null,
  creator: null,
  producer: null,
  created: "2024-03-01T10:00:00",
  modified: null,
  lang: "en",
  pages: 12,
  pages_read: 12,
  words: 3400,
  encrypted: false,
  tagged: true,
  no_text: false,
  links: 3,
  text_stored: true,
  ...p,
});

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
    columns: [col("address", "Address", "url", 420), col("status_code", "Status code", "status", 96), col("title", "Title", "text", 320), col("h1", "H1", "text", 260, "extraction"), col("depth", "Depth", "int", 70, "base", false), col("segment", "Segment", "text", 140, "base", false)],
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
    columns: [col("address", "Address", "url", 420), col("status_code", "Status code", "status", 96), col("segment", "Segment", "text", 140, "base", false)],
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
  {
    id: "pdfs",
    title: "PDFs",
    columns: [col("address", "Address", "url", 420), col("pdf_title", "PDF title", "text", 280, "pdf"), col("pdf_pages", "Pages", "int", 80, "pdf"), col("pdf_status", "Read", "text", 140, "pdf")],
    filters: [
      { id: "all", label: "All", need: "base" },
      { id: "missing_title", label: "Missing title", need: "pdf" },
    ],
    search_column: "pdf_title",
    scoped: false,
  },
  {
    id: "spelling",
    title: "Spelling",
    columns: [col("address", "Address", "url", 420), col("spell_errors", "Spelling errors", "int", 110, "spelling")],
    filters: [
      { id: "all", label: "All", need: "base" },
      { id: "errors", label: "Spelling errors", need: "spelling" },
    ],
    search_column: null,
    scoped: false,
  },
  {
    id: "amp",
    title: "AMP",
    columns: [col("address", "Address", "url", 420, "amp"), col("amp_failures", "Failed checks", "int", 110, "amp")],
    filters: [
      { id: "all", label: "All", need: "amp" },
      { id: "failing", label: "Failing a check", need: "amp" },
    ],
    search_column: null,
    scoped: false,
  },
  {
    id: "sitemaps",
    title: "Sitemaps",
    columns: [col("address", "Address", "url", 420), col("status_code", "Status code", "int", 90)],
    filters: [
      { id: "all", label: "All sitemap URLs", need: "base" },
      { id: "orphans", label: "Orphan pages", need: "base" },
    ],
    search_column: null,
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

/**
 * A small stand-in for the engine's segment matcher (the real one is tested
 * in Rust): text conditions on the URL, host and path, and status class.
 */
export function fakeSegmentOf(segments: Segment[], u: FakeUrl): Segment | null {
  const parsed = new URL(u.url);
  const facts: Record<string, string> = {
    url: u.url,
    host: parsed.host,
    path: parsed.pathname,
    query: parsed.search.replace(/^\?/, ""),
    status_class: u.status === null ? "none" : `${Math.floor(u.status / 100)}xx`,
  };
  const test = (c: Segment["clauses"][number]) => {
    const hay = (facts[c.field] ?? "").toLowerCase();
    const v = c.value.trim().toLowerCase();
    switch (c.op) {
      case "contains":
        return hay.includes(v);
      case "not_contains":
        return !hay.includes(v);
      case "starts_with":
        return hay.startsWith(v);
      case "ends_with":
        return hay.endsWith(v);
      case "equals":
        return hay === v;
      case "not_equals":
        return hay !== v;
      case "matches":
        return new RegExp(c.value).test(facts[c.field] ?? "");
      default:
        return false;
    }
  };
  return segments.find((s) => s.enabled && s.clauses.length > 0 && (s.mode === "all" ? s.clauses.every(test) : s.clauses.some(test))) ?? null;
}

/** A made-up Link Score: the start URL is the strongest page. */
const fakeScore = (u: FakeUrl) => (u.id === 1 ? 100 : ((u.id * 37) % 1000) / 10);
const cell = (u: FakeUrl, c: string, segments: Segment[] = [], urls: FakeUrl[] = []) => {
  const d = nd(u.id);
  const closest = d ? urls.find((x) => x.id === d.others[0].o) : undefined;
  return (
    {
      main_words: u.title === null ? null : 240,
      closest_sim: d ? d.others[0].sim : null,
      closest_url: closest?.url ?? null,
      near_count: d ? d.others.length : 0,
      cluster: d?.cluster ?? null,
      address: u.url, status_code: u.status, title: u.title, h1: u.h1 ?? null, depth: u.depth, link_score: fakeScore(u), followed_inlinks: u.id % 7, segment: fakeSegmentOf(segments, u)?.name ?? null, pdf_title: u.pdf?.title ?? null, pdf_pages: u.pdf?.pages ?? null, pdf_status: u.pdf?.status_label ?? null, spell_errors: u.spellErrors ?? null, amp_failures: u.ampFailures ?? null,
    } as Record<string, string | number | null>
  )[c] ?? null;
};

/** The JavaScript tab (spec 014). Not in `fakeTabs`: the fake lists it only for rendered crawls, or when a test asks (`results.javascriptTab`). */
export const fakeJavascriptTab: TabDef = {
  id: "javascript",
  title: "JavaScript",
  columns: [col("address", "Address", "url", 420), col("render_status", "Rendering status", "text", 190, "rendering"), col("words_change", "Word count change", "int", 140, "rendering")],
  filters: [
    { id: "all", label: "All", need: "rendering" },
    { id: "js_content", label: "Contains JavaScript content (over 20% and 50 words)", need: "rendering" },
  ],
  search_column: null,
  scoped: false,
};

/** A rendered page's detail with a comparison in which nothing changed. */
export const fakeRender = (patch: Partial<RenderDetail> = {}): RenderDetail => {
  const same = { diff: "same", raw: null, rendered: null };
  return {
    state: "rendered",
    reason: null,
    reason_label: null,
    reason_detail: null,
    attempts: 1,
    render_ms: 420,
    window_w: 1024,
    window_h: 768,
    js_redirect: null,
    js_redirect_id: null,
    console_errors: 0,
    console_warnings: 0,
    resources_failed: 0,
    resources_blocked: 0,
    bot_challenge: false,
    comparison: {
      words_raw: 50,
      words_rendered: 50,
      words_change: 0,
      links_raw: 3,
      links_rendered: 3,
      links_rendered_only: 0,
      links_raw_only: 0,
      title: same,
      description: same,
      h1: same,
      canonical: same,
      noindex: same,
      nofollow: same,
      hreflang: same,
      structured_rendered_only: 0,
      dependence: 0,
      dep_text: 0,
      dep_headings: null,
      dep_internal_links: 0,
      dep_structured: null,
      dep_images: null,
      raw_has_title: true,
      raw_has_description: true,
      raw_has_canonical: true,
      raw_has_directives: false,
    },
    rendered_html_stored: true,
    screenshot_stored: false,
    ...patch,
  };
};

/** A cookie the fake crawl saw, and the URLs that set it. */
export interface FakeCookie extends Omit<CookieRow, "urls"> {
  setBy: number[];
}

const cookieMatches: Record<CookieFilter, (c: FakeCookie) => boolean> = {
  all: () => true,
  session: (c) => c.session,
  persistent: (c) => !c.session,
  third_party: (c) => c.third_party,
  missing_secure: (c) => !c.secure,
  missing_http_only: (c) => !c.http_only,
  same_site_none: (c) => c.same_site === "none",
};

const cookieRow = ({ setBy, ...c }: FakeCookie): CookieRow => ({ ...c, urls: setBy.length });

export function fakeCookie(name: string, setBy: number[], c: Partial<FakeCookie> = {}): FakeCookie {
  return { name, domain: "example.com", path: "/", session: true, expires: null, secure: true, http_only: true, same_site: "lax", third_party: false, source: "response", size: name.length + 20, setBy, ...c };
}

export interface FakeResults {
  urls: FakeUrl[];
  /** Cookies the crawl recorded; seen only when features include "cookies". */
  cookies: FakeCookie[];
  copied: string[];
  exports: { columns: string[]; ranges: Range[] | null; dest: string; allValues: string | null; segments?: Segment[] }[];
  opened: { tab: string; filter: string; search: string; sort: string | null; segment?: number; values?: string }[];
  /** The `values` of each counts call, in order (spec 014). */
  counted: (string | undefined)[];
  /** How often the sidebar asked for its tab counts. */
  tabCountCalls: number;
  /** Segments applied to the open crawl (segmentsSet). */
  segments: Segment[];
  segmentCalls: Segment[][];
  /** What the open crawl file holds; base only unless a test adds more. */
  features: Need[];
  /** Announces that the open view's full order is ready. */
  ready(): void;
  progress(p: Partial<ExportProgress>): void;
  /** Visualisation states asked for (spec 022). */
  viz: ViewState[];
  /** Visualisation exports asked for, in order. */
  vizExports: { view: ViewState; options: ExportOptions; file: string }[];
  /** Adds URLs, as a running crawl would; the next refresh sees them. */
  add(urls: FakeUrl[]): void;
  /** Near duplicate analysis state; "done" once a test or a run says so. */
  nearDup: NearDupStatus["state"];
  /** Whether the file keeps HTML for the comparison view. */
  htmlStored: boolean;
  /** List the JavaScript tab even when the crawl was not rendered (spec 014 FR-034). */
  javascriptTab: boolean;
  /** Rendering detail by URL id (spec 014); pages without an entry were not rendered. */
  renders: Map<number, RenderDetail>;
  /** What `urlHtmlDiff` answers (spec 014 FR-028); null rejects, as when a version is not stored. */
  htmlDiff: HtmlDiff | null;
  analysisRuns: number;
  nearDupExports: { kind: string; cluster: number | null; dest: string }[];
  /** Ends a running analysis, as the engine would. */
  finishAnalysis(f?: Partial<NearDupFinished>): void;
  nearDupProgress(p: Partial<NearDupProgress>): void;
  /** The open file's custom tabs (spec 013); none unless a test adds them. */
  customTabs: TabDef[];
  /** Custom column values by URL id; pages without an entry are not in the custom tabs. */
  customCells: Map<number, Record<string, string | number | null>>;
  /** Link analysis settings sent for the open file (spec 023). */
  linkSettings: LinkAnalysisSettings[];
  /** Crawl Analysis runs started (file paths) and cancels asked for. */
  analyses: string[];
  analysisCancels: number;
  /** Reports progress of the running analysis. */
  analysisProgress(p: Partial<AnalysisProgress>): void;
  /** Ends the running analysis: with a run, or cancelled or failed. */
  analysisDone(d?: Partial<AnalysisDone>): void;

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
  | "segmentsSet"
  | "segmentsCounts"
  | "viewClose"
  | "urlDetail"
  | "urlList"
  | "vizScene"
  | "vizTermLinks"
  | "vizExport"
  | "cookiesList"
  | "cookieUrls"
  | "urlHtml"
  | "urlHtmlDiff"
  | "urlTabs"
  | "copyText"
  | "copyRows"
  | "exportStart"
  | "exportCancel"
  | "analysisRun"
  | "analysisSettings"
  | "analysisStart"
  | "analysisCancel"
  | "onAnalysisProgress"
  | "onAnalysisDone"
  | "positionsTest"
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
  | "onNearDupProgress"
  | "onNearDupFinished"
  | "ruleCatalogue"
  | "issuesApply"
  | "issuesSummary"
  | "onIssuesPartial"
  | "urlIssues"
> & { results: FakeResults; issues: FakeIssues } {
  let version = 1;
  let next = 0;
  const views = new Map<number, { tab: TabDef; ids: number[]; version: number }>();
  const readyCbs = new Set<(r: ViewReady) => void>();
  const progressCbs = new Set<(p: ExportProgress) => void>();
  const nearDupProgressCbs = new Set<(p: NearDupProgress) => void>();
  const nearDupFinishedCbs = new Set<(f: NearDupFinished) => void>();
  const analysisProgressCbs = new Set<(p: AnalysisProgressEvent) => void>();
  const analysisDoneCbs = new Set<(d: AnalysisDone) => void>();
  let analysing: string | null = null;
  const fakeRun = (): AnalysisRun => ({ urls: results.urls.length, links: 12, rounds: 30, pages_without_html: 0, weighted: false, ms: 40 });
  const results: FakeResults = {
    urls: [...initial],
    cookies: [],
    copied: [],
    viz: [],
    vizExports: [],
    linkSettings: [],
    analyses: [],
    analysisCancels: 0,
    analysisProgress: (p) => analysisProgressCbs.forEach((cb) => cb({ path: analysing ?? "", phase: "positions", done: 0, total: 100, ...p })),
    analysisDone: (d) => {
      const path = analysing ?? "";
      analysing = null;
      analysisDoneCbs.forEach((cb) => cb({ path, run: d?.cancelled || d?.error ? null : fakeRun(), cancelled: false, error: null, ...d }));
    },
    exports: [],
    opened: [],
    counted: [],
    tabCountCalls: 0,
    segments: [],
    segmentCalls: [],
    features: ["base", "analysis"],
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
    javascriptTab: false,
    renders: new Map(),
    htmlDiff: null,
    analysisRuns: 0,
    nearDupExports: [],
    finishAnalysis: (f) => {
      results.nearDup = f?.error || f?.cancelled ? "not_run" : "done";
      if (results.nearDup === "done" && !results.features.includes("near_dup")) results.features.push("near_dup");
      version++;
      nearDupFinishedCbs.forEach((cb) =>
        cb({ path: "/crawls/example.com.spiderlens", summary: f?.error || f?.cancelled ? null : { compared: 27, too_short: 0, not_indexable: 0, no_signature: 0, clusters: 2, ms: 40 }, cancelled: false, error: null, ...f }),
      );
    },
    nearDupProgress: (p) => nearDupProgressCbs.forEach((cb) => cb({ path: "/crawls/example.com.spiderlens", done: 1, total: 3, step: "Comparing pages", ...p })),
    customTabs: [],
    customCells: new Map(),
  };
  const issuesApi = fakeIssuesTransport(() => results.urls);
  const isCustom = (tab: string) => results.customTabs.some((t) => t.id === tab);
  const tabOf = (id: string) => [...fakeTabs, fakeJavascriptTab, ...fakeIssueTabs].find((t) => t.id === id) ?? results.customTabs.find((t) => t.id === id)!;
  const valueOf = (u: FakeUrl, c: string) => (results.customCells.get(u.id)?.[c] ?? cell(u, c, results.segments, results.urls)) as ReturnType<typeof cell>;
  const queries = new Map<number, Parameters<Transport["viewOpen"]>[1]>();
  const select = (q: Parameters<Transport["viewOpen"]>[1]) => {
    const tab = tabOf(q.tab);
    const rule = q.tab.startsWith("issue:") ? issuesApi.issues.rules().find((r) => `issue:${r.id}` === q.tab) : undefined;
    let us = rule ? hits(rule, results.urls) : results.urls.filter((u) => (q.scope === "all" ? true : (q.scope === "internal") === u.internal));
    if (isCustom(q.tab)) {
      us = us.filter((u) => results.customCells.has(u.id));
      // `<column>_has`: a value; `<column>_none`: none.
      const [, col, kind] = /^(.*)_(has|none)$/.exec(q.filter) ?? [];
      if (col) us = us.filter((u) => (valueOf(u, col) !== null && valueOf(u, col) !== "") === (kind === "has"));
    }
    if (q.segment !== undefined && q.segment !== null) us = us.filter((u) => (fakeSegmentOf(results.segments, u)?.id ?? 0) === q.segment);
    if (q.tab === "pdfs") us = us.filter((u) => u.pdf);
    if (q.tab === "pdfs" && q.filter === "missing_title") us = us.filter((u) => !u.pdf?.title);
    if (q.filter === "errors") us = us.filter((u) => (u.spellErrors ?? 0) > 0);
    if (q.tab === "javascript") us = us.filter((u) => results.renders.has(u.id));
    if (q.tab === "amp") us = us.filter((u) => u.ampFailures !== undefined);
    if (q.tab === "sitemaps") us = us.filter((u) => u.internal);
    if (q.tab === "sitemaps" && q.filter === "orphans") us = us.filter((u) => u.id % 50 === 7);
    if (q.tab === "amp" && q.filter === "failing") us = us.filter((u) => (u.ampFailures ?? 0) > 0);
    if (q.filter === "html") us = us.filter((u) => u.title !== null);
    if (q.filter === "missing_h1") us = us.filter((u) => u.title !== null && !u.h1);
    if (q.filter === "depth_4") us = us.filter((u) => u.depth >= 4);
    if (q.filter === "class_timeout") us = [];
    if (q.filter === "class_http_404") us = us.filter((u) => u.status === 404);
    if (q.filter === "client_error") us = us.filter((u) => u.status !== null && u.status >= 400 && u.status < 500);
    if (q.tab === "content") us = us.filter((u) => u.internal && u.status === 200);
    if (q.filter === "near_duplicates") us = us.filter((u) => clusterOf(u.id) >= 0);
    const s = q.search.toLowerCase();
    if (s) us = us.filter((u) => u.url.toLowerCase().includes(s) || (u.title ?? "").toLowerCase().includes(s));
    if (q.sort) {
      const k = q.sort.column;
      const d = q.sort.desc ? -1 : 1;
      us = [...us].sort((a, b) => {
        const x = valueOf(a, k);
        const y = valueOf(b, k);
        if (x === y) return a.id - b.id;
        if (x === null) return 1;
        if (y === null) return -1;
        return (x < y ? -1 : 1) * d;
      });
    }
    return { tab, ids: us.map((u) => u.id) };
  };
  const segmentCounts = (): SegmentCounts => {
    const counts: Record<number, number> = {};
    for (const s of results.segments) if (s.enabled) counts[s.id] = 0;
    let none = 0;
    for (const u of results.urls) {
      const s = fakeSegmentOf(results.segments, u);
      if (s) counts[s.id] += 1;
      else none += 1;
    }
    return { counts, no_segment: none, segments: results.segments };
  };
  const row = (tab: TabDef, id: number) => {
    const u = results.urls.find((x) => x.id === id)!;
    return [id, ...tab.columns.map((c) => (results.features.includes(c.need) ? valueOf(u, c.id) : null))];
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
    onNearDupProgress: async (cb) => {
      nearDupProgressCbs.add(cb);
      return () => void nearDupProgressCbs.delete(cb);
    },
    onNearDupFinished: async (cb) => {
      nearDupFinishedCbs.add(cb);
      return () => void nearDupFinishedCbs.delete(cb);
    },
    ...issuesApi,
    resultsTabs: async (path) => {
      const js = results.javascriptTab || results.features.includes("rendering") ? [fakeJavascriptTab] : [];
      return path ? [...fakeTabs, ...js, ...fakeIssueTabs, ...results.customTabs] : [...fakeTabs, ...js, ...fakeIssueTabs];
    },
    resultsOpen: async () => ({ features: { available: [...results.features], redirect_limit: 10 } }),
    viewOpen: async (_p, q, count) => {
      const view = ++next;
      results.opened.push({
        tab: q.tab,
        filter: q.filter,
        search: q.search,
        sort: q.sort ? `${q.sort.column}${q.sort.desc ? ":desc" : ""}` : null,
        ...(q.values ? { values: q.values } : {}),
        ...(q.segment !== undefined && q.segment !== null ? { segment: q.segment } : {}),
      });
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
    segmentsSet: async (_p, segments) => {
      results.segments = segments;
      results.segmentCalls.push(segments);
      version++;
      return segmentCounts();
    },
    segmentsCounts: async () => segmentCounts(),
    viewCounts: async (_p, tab, scope, segment, values) => {
      results.counted.push(values);
      const t = tabOf(tab);
      const counts: Record<string, number | null> = {};
      const unavailable: Record<string, string> = {};
      for (const f of t.filters) {
        if (!results.features.includes(f.need)) {
          counts[f.id] = null;
          unavailable[f.id] = "Not collected yet: arrives with page extraction.";
        } else counts[f.id] = select({ tab, filter: f.id, scope, search: "", sort: null, segment }).ids.length;
      }
      return { tab, counts, unavailable, labels: {}, version };
    },
    tabCounts: async () => {
      results.tabCountCalls++;
      return { counts: Object.fromEntries(fakeTabs.map((t) => [t.id, select({ tab: t.id, filter: "all", scope: "all", search: "", sort: null }).ids.length])), version };
    },
    viewClose: async (_p, view) => {
      views.delete(view);
    },
    vizScene: async (_p, state) => {
      results.viz.push(state);
      return fakeScene(results.urls, state);
    },
    vizTermLinks: async (_p, state, term, limit) => fakeTermLinks(results.urls, state, term, limit ?? 500),
    vizExport: async (_p, view, options, file) => {
      results.vizExports.push({ view, options, file });
      if (file.includes("too-big")) throw new Error("the picture would be 20000 x 900 pixels, over the 16000 pixel limit: export it as SVG, or export a focused view");
      return { files: options.format === "csv" ? [file, file.replace(/\.csv$/, "-links.csv")] : [file] };
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
        content_type: u.pdf ? "application/pdf" : "text/html",
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
        html_stored: !u.pdf && results.features.includes("raw_html") && u.title !== null,
        pdf: u.pdf && results.features.includes("pdf") ? u.pdf : null,
        snippet: u.status === 200 ? { title: u.title, description: u.description ?? null } : null,
        link_metrics: null,
        spelling: null,
        render: results.renders.get(id) ?? null,
        amp: null,
        accessibility: null,
        mobile: null,
        attempts: results.features.includes("attempts") ? (u.attempts ?? 1) : null,
        error_reason: null,
        manual_retry: u.manual_retry ?? false,
        cookies: results.features.includes("cookies") ? results.cookies.filter((c) => c.setBy.includes(id)).map((c) => ({ ...cookieRow(c), urls: 1 })) : [],
        unavailable: {
          ...(results.features.includes("attempts") ? {} : { attempts: "Request attempts are not recorded in this crawl yet." }),
          ...(results.features.includes("raw_html") ? {} : { raw_html: "This crawl does not store page HTML." }),
          ...(results.features.includes("cookies") ? {} : { cookies: "This crawl was made before SpiderLens recorded the cookies each page sets." }),
        },
      };
      return d;
    },
    urlList: async (_p, id, kind, from, _count, position) =>
      kind === "inlinks"
        ? (() => {
            const rows = [
              { id: 1, url: results.urls[0].url, raw: "/", kind: "a", nofollow: false, followed: true, internal: true, status: 200, final_state: "Crawled", text: "Home", sponsored: false, ugc: false, position: "navigation", origin: null },
              { id: 2, url: results.urls[1 % results.urls.length].url, raw: "/story", kind: "a", nofollow: false, followed: true, internal: true, status: 200, final_state: "Crawled", text: "Read on", sponsored: false, ugc: false, position: "content", origin: null },
            ].filter((r) => id !== -1 && (!position || r.position === position));
            return { total: rows.length, from, rows, headings: [], resources: [], console: [], unavailable: null };
          })()
        : kind === "headings"
          ? { total: 2, from, rows: [], headings: [{ level: 1, text: `Heading for ${id}`, origin: null }, { level: 2, text: "Section one", origin: null }], resources: [], console: [], unavailable: null }
          : kind === "images"
            ? {
                total: 2,
                from,
                rows: [null, ""].map((text, i) => ({ id: 100 + i, url: `https://example.com/img-${i}.png`, raw: `/img-${i}.png`, kind: "image", nofollow: false, followed: true, internal: true, status: 200, final_state: "Crawled", text, sponsored: false, ugc: false, position: null, origin: null })),
                headings: [],
                resources: [],
                console: [],
                unavailable: null,
              }
            : { total: 0, from, rows: [], headings: [], resources: [], console: [], unavailable: null },
    urlHtmlDiff: async (_p, _id, fromHunk, maxHunks) => {
      if (!results.htmlDiff) throw new Error("The rendered HTML of this page is not stored in this crawl.");
      return { ...results.htmlDiff, hunks: results.htmlDiff.hunks.slice(fromHunk, fromHunk + maxHunks) };
    },
    urlHtml: async (_p, id, version) => {
      const u = results.urls.find((x) => x.id === id)!;
      const none = { html: null, bytes: 0, encoding: null, encoding_errors: false };
      if (version === "rendered") {
        const html = `<!doctype html><html><head><title>Rendered ${id}</title></head><body></body></html>`;
        return results.renders.get(id)?.rendered_html_stored ? { html, missing: null, bytes: html.length, encoding: "utf-8", encoding_errors: false } : { ...none, missing: "not_stored" };
      }
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
    cookiesList: async (_p, filter, sort, descending, offset, limit) => {
      if (!results.features.includes("cookies")) throw new Error("This crawl file has no cookie data.");
      const rows = results.cookies.filter(cookieMatches[filter]).map(cookieRow);
      const key = (sort ?? "urls") as keyof CookieRow;
      const down = descending || sort === null;
      rows.sort((a, b) => {
        const x = a[key] ?? "";
        const y = b[key] ?? "";
        return (x < y ? -1 : x > y ? 1 : 0) * (down ? -1 : 1) || a.name.localeCompare(b.name);
      });
      const counts = Object.fromEntries(Object.entries(cookieMatches).map(([f, m]) => [f, results.cookies.filter(m).length]));
      return { rows: rows.slice(offset, offset + limit), total: rows.length, counts };
    },
    cookieUrls: async (_p, name, domain, cookiePath, offset, limit) => {
      const c = results.cookies.find((x) => x.name === name && x.domain === domain && x.path === cookiePath);
      const ids = c?.setBy ?? [];
      const rows = ids.slice(offset, offset + limit).map((id) => {
        const u = results.urls.find((x) => x.id === id)!;
        return { id, url: u.url, status: u.status };
      });
      return { rows, total: ids.length };
    },
    exportStart: async (_p, _q, columns, ranges, dest, allValues, segments) => {
      results.exports.push({ columns, ranges, dest, allValues: allValues ?? null, ...(segments ? { segments } : {}) });

      return 1;
    },
    exportCancel: async () => {},
    analysisRun: async () => fakeRun(),
    analysisStart: async (path) => {
      analysing = path;
      results.analyses.push(path);
    },
    analysisCancel: async () => {
      results.analysisCancels++;
    },
    onAnalysisProgress: async (cb) => {
      analysisProgressCbs.add(cb);
      return () => void analysisProgressCbs.delete(cb);
    },
    onAnalysisDone: async (cb) => {
      analysisDoneCbs.add(cb);
      return () => void analysisDoneCbs.delete(cb);
    },
    analysisSettings: async (_p, settings) => {
      results.linkSettings.push(settings);
    },
    // The page has a header link, a menu link and a content link; the first
    // rule, when there is one, claims the menu link.
    positionsTest: async (_p, url, rules) => {
      if (!results.urls.some((u) => u.url === url.trim())) return null;
      const rule = rules[0];
      return [
        { pos: 0, raw: "/", text: "Home", position: "header" as const, rule: null },
        { pos: 1, raw: "/promo", text: "Promo", position: rule ? rule.position : ("content" as const), rule: rule ? 0 : null },
        { pos: 2, raw: "/story", text: "Read the story", position: "content" as const, rule: null },
      ];
    },
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
