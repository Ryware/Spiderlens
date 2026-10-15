// In-memory Transport for UI tests. Validation here is a small stand-in; the
// real rules are tested in Rust (spiderlens-config).

import type { PassProgress } from "../generated/PassProgress";
import type { CrawlConfig } from "../generated/CrawlConfig";
import type { Issue } from "../generated/Issue";
import type { GenerateReport } from "../generated/GenerateReport";
import type { SitemapFileRow } from "../generated/SitemapFileRow";
import type { SitemapFiles } from "../generated/SitemapFiles";
import type { SitemapGenerateOptions } from "../generated/SitemapGenerateOptions";
import type { RetrySettings } from "../generated/RetrySettings";
import type { SpeedSettings } from "../generated/SpeedSettings";
import { fakeResultsTransport, fakeUrls, type FakeResults, type FakeUrl } from "./fakeResults";
import { fakeAgentsTransport, type FakeAssistants } from "./fakeAgents";
import { fakeCompareTransport, type FakeCompare } from "./fakeCompare";
import { fakeIntegrationsTransport, type FakeIntegrations } from "./fakeIntegrations";
import { fakeAeoTransport, type FakeAeo } from "./fakeAeo";
import { fakeResultsTransport, fakeSegmentOf, fakeUrls, type FakeResults, type FakeUrl } from "./fakeResults";
import { fakeAuditTransport, type FakeAudit } from "./fakeAudit";
import type { FakeIssues } from "./fakeIssues";
import { fakeCustomTransport, type FakeCustom } from "./fakeCustom";
import { fakeSpellingTransport, type FakeSpelling } from "./fakeSpelling";
import { fakeSchedulesTransport, type FakeSchedules } from "./fakeSchedules";

import type {
  CrawlFinished,
  CrawlInfo,
  CrawlMoved,
  CrawlNotice,
  CrawlProgress,
  CrawlReport,
  CrawlStatus,
  LoadedProfile,
  LocalError,
  OpenFailure,
  Presets,
  CleanupRecord,
  CleanupReport,
  CrawlBackup,
  HtmlSize,
  ManagedCrawl,
  RetryPreview,
  RecentEntry,
  ResumeOptions,
  StoragePrefs,
  StorageStatus,
  Transport,
} from "../transport";

const polite: SpeedSettings = { max_concurrent: 8, max_per_host: 4, max_urls_per_sec: 5, adaptive: true };
const fast: SpeedSettings = { max_concurrent: 64, max_per_host: 32, max_urls_per_sec: null, adaptive: true };

/** The engine's defaults for sitemap generation (spec 011 Assumptions). */
export function defaultGenerateOptions(): SitemapGenerateOptions {
  return {
    include_noindex: false,
    include_canonicalised: false,
    include: [],
    exclude: [],
    lastmod: { kind: "last_modified_header" },
    changefreq: null,
    priority: { kind: "off" },
    images: "off",
    image_hosts: [],
    hreflang: false,
    max_urls_per_file: 50000,
    gzip: false,
    base_url: "",
    file_prefix: "sitemap",
  };
}

export function defaultConfig(): CrawlConfig {
  return {
    mode: "spider",
    limits: { max_urls: null, max_depth: null, list_depth: 0 },
    scope: {
      include: [],
      exclude: [],
      subdomains: "this_host",
      stay_in_folder: false,
      check_external: true,
      follow_internal_nofollow: false,
      follow_external_nofollow: false,
      query_params: { mode: "keep", names: [], sort: false },
      max_url_length: 2000,
      sitemaps: { read: true, crawl_sitemap_only: true, urls: [], files: [] },
    },
    identity: { user_agent: { preset: "spiderlens", custom_string: null, robots_token: null }, headers: [] },
    speed: { ...polite, preset: "polite" },
    robots: { mode: "respect", honour_crawl_delay: true },
    network: { connect_timeout_secs: 10, read_timeout_secs: 30, max_redirects: 10, max_body_mb: 50 },
    retry: {
      attempts: 3,
      max_wait_secs: 60,
      retry_5xx: true,
      retry_429: true,
      base_wait_secs: 1,
      host_pause_after: 5,
      host_pause_secs: 60,
      host_pause_max_secs: 600,
      host_give_up_after: 5,
    },
    extraction: { store_html: true, check_amp: false, pdf: { crawl: false, follow_links: true, max_mb: 20, time_limit_secs: 5 } },
    content: { threshold: 90, report_floor: 80, min_words: 50, indexable_only: true, content_selector: null, exclude_selectors: [] },
    issues: { rules: {} },
    custom: { extractors: [], searches: [] },
    sitemap_generate: defaultGenerateOptions(),
    links: { auto_analyse: true, position_rules: [], weighting: null },
    access: { cookies: [], cookie_policy: "ignore" },
    ai_checks: { auto: true, site_files: true },
    segments: [],
    spelling: { check: false, default_lang: "en", skip_all_caps: true, ignore: [] },
    rendering: {
      mode: "raw",
      max_render_secs: 10,
      min_wait_secs: 0,
      window: { preset: "auto", width: null, height: null },
      block: { images: true, media: true, fonts: true, stylesheets: false, third_party: false },
      include: [],
      exclude: [],
      render_error_pages: false,
      concurrency: null,
      memory_budget_mb: null,
      screenshots: false,
      js_content_min_percent: 20,
      js_content_min_words: 50,
      hidden_content_percent: 50,
    },
  };
}

const same = (a: SpeedSettings, b: SpeedSettings) =>
  a.max_concurrent === b.max_concurrent &&
  a.max_per_host === b.max_per_host &&
  a.max_urls_per_sec === b.max_urls_per_sec &&
  a.adaptive === b.adaptive;

const presets: Presets = {
  user_agents: [
    { preset: "spiderlens", label: "SpiderLens", user_agent: "SpiderLens/0.1.0", robots_token: "SpiderLens" },
    { preset: "googlebot_smartphone", label: "Googlebot Smartphone", user_agent: "Googlebot", robots_token: "Googlebot" },
    { preset: "custom", label: "Custom", user_agent: null, robots_token: null },
  ],
  polite,
  fast,
};

/** A finished-crawl report with the given figures, for tests. */
export function fakeReport(status: CrawlStatus, crawled: number, path = "/crawls/example.com.spiderlens"): CrawlReport {
  return {
    path,
    summary: {
      status,
      discovered: crawled + 3,
      crawled,
      not_crawled: 3,
      queued: 0,
      by_reason: { external: 2, blocked_by_robots: 1 },
      external_checked: 2,
      external_blocked: 0,
      url_limit_reached: false,
      hosts: [{ host: "example.com", robots: "ok", crawl_delay_secs: null, error: null }],
      sitemaps: [],
      warnings: [],
      by_class: {},
      transient_errors: 0,
      permanent_errors: 0,
      retries: 0,
      recovered: 0,
      hosts_paused: 0,
      manual_retries: 0,
      pause_reason: null,
    },
    reasons: [
      { reason: "external", label: "External", count: 2 },
      { reason: "blocked_by_robots", label: "Blocked by robots.txt", count: 1 },
    ],
  };
}

type Listeners<T> = Set<(v: T) => void>;

/** What a test can see of crawls and how it drives their events. */
export interface FakeCrawls {
  started: { config: CrawlConfig; startUrl: string | null }[];
  resumed: string[];
  controls: string[];
  /** "Resume now" calls, as "id host". */
  resumedHosts: string[];
  /** Settings sent to a running crawl. */
  applied: { id: number; speed: SpeedSettings | null; retry: RetrySettings | null }[];
  /** Files "Retry failed URLs" ran on. */
  retried: string[];
  /** What retry_preview answers; tests replace it. */
  preview: RetryPreview;
  progress(p: Partial<CrawlProgress>): void;
  warn(message: string): void;
  analyse(p: Partial<PassProgress>): void;
  /** Files `passesRun` was called for. */
  analysed: string[];
  notice(n: Partial<CrawlNotice> & { message: string }): void;
  finish(f: Partial<CrawlFinished>): void;
}

/** What the fake file pickers return; tests set these before clicking. */
export interface FakePicks {
  sitemapFile: string | null;
  folder: string | null;
}

/** A sitemap file row with defaults, for tests. */
export function fakeSitemapFile(id: number, location: string, patch: Partial<SitemapFileRow> = {}): SitemapFileRow {
  return {
    id,
    location,
    source: "robots",
    parent_id: null,
    level: 0,
    kind: "urlset",
    http_status: 200,
    final_url: location,
    redirected: false,
    compressed: false,
    bytes: 1200,
    entries: 10,
    over_limit: false,
    error_class: null,
    error: null,
    error_line: null,
    invalid_entries: 0,
    children: 0,
    base_url: null,
    ...patch,
  };
}

/** Sitemap generation and files as the tests see them. */
export interface FakeSitemaps {
  /** What the Sitemap files view reads. */
  files: SitemapFiles;
  /** File names already in each folder. */
  existing: Map<string, string[]>;
  generated: { folder: string; options: SitemapGenerateOptions; replace: boolean }[];
  previews: number;
  /** When set, the next generate fails with this message. */
  fail: string | null;
}

/** A stand-in for the engine's report: internal 200 pages, split by the URL limit. */
function fakeReport_(urls: FakeUrl[], o: SitemapGenerateOptions, existingIn: string[]): GenerateReport {
  const re = (p: string) => {
    try {
      return new RegExp(p);
    } catch {
      return null;
    }
  };
  const kept = urls.filter((u) => u.internal && u.status === 200);
  const pages = kept.filter(
    (u) => (o.include.length === 0 || o.include.some((p) => re(p)?.test(u.url))) && !o.exclude.some((p) => re(p)?.test(u.url)),
  );
  const ext = o.gzip ? ".xml.gz" : ".xml";
  const n = Math.ceil(pages.length / Math.max(1, o.max_urls_per_file));
  const files =
    pages.length === 0
      ? []
      : n === 1
        ? [{ name: `${o.file_prefix}${ext}`, kind: "pages" as const, urls: pages.length, images: 0, bytes: pages.length * 80 }]
        : [
            { name: `${o.file_prefix}_index${ext}`, kind: "index" as const, urls: n, images: 0, bytes: n * 90 },
            ...Array.from({ length: n }, (_, i) => {
              const c = Math.min(o.max_urls_per_file, pages.length - i * o.max_urls_per_file);
              return { name: `${o.file_prefix}-${i + 1}${ext}`, kind: "pages" as const, urls: c, images: 0, bytes: c * 80 };
            }),
          ];
  return {
    files,
    pages: pages.length,
    images: 0,
    skipped_by_pattern: kept.length - pages.length,
    existing: files.map((f) => f.name).filter((name) => existingIn.includes(name)),
    base_url: o.base_url || "https://example.com/",
  };
}

export function fakeTransport(
  urls: FakeUrl[] = fakeUrls(30),
): Transport & { saved: Map<string, LoadedProfile>; crawls: FakeCrawls; results: FakeResults; files: FakeFiles; assistants: FakeAssistants } {
): Transport & { saved: Map<string, LoadedProfile>; crawls: FakeCrawls; results: FakeResults; files: FakeFiles; picks: FakePicks; sitemaps: FakeSitemaps; compare: FakeCompare; aeo: FakeAeo; schedules: FakeSchedules; issues: FakeIssues; integrations: FakeIntegrations; custom: FakeCustom; audit: FakeAudit; spelling: FakeSpelling } {
  const picks: FakePicks = { sitemapFile: null, folder: null };
  const sitemaps: FakeSitemaps = { files: { available: true, files: [] }, existing: new Map(), generated: [], previews: 0, fail: null };

  const saved = new Map<string, LoadedProfile>();
  let listCount = 0;
  let nextId = 1;
  const onProgress: Listeners<CrawlProgress> = new Set();
  const onWarning: Listeners<{ id: number; message: string }> = new Set();
  const onFinished: Listeners<CrawlFinished> = new Set();
  const onAnalysis: Listeners<PassProgress> = new Set();
  const onNotice: Listeners<CrawlNotice> = new Set();
  const subscribe = async <T>(set: Listeners<T>, cb: (v: T) => void) => {
    set.add(cb);
    return () => void set.delete(cb);
  };
  const crawls: FakeCrawls = {
    started: [],
    resumed: [],
    controls: [],
    resumedHosts: [],
    applied: [],
    retried: [],
    preview: { total: 0, by_class: [], available: false, reason: "No URL failed for a reason that could change." },
    progress: (p) =>
      onProgress.forEach((cb) =>
        cb({
          id: nextId - 1,
          discovered: 0,
          crawled: 0,
          in_flight: 0,
          queued: 0,
          not_crawled: 0,
          errors: 0,
          urls_per_sec: 0,
          avg_urls_per_sec: 0,
          eta_secs: null,
          active_ms: 0,
          waiting: 0,
          paused_hosts: [],
          waiting_for_host: false,
          limit: null,
          sites: [],
          sites_total: 0,
          ...p,
        }),
      ),
    warn: (message) => onWarning.forEach((cb) => cb({ id: nextId - 1, message })),
    analyse: (p) => onAnalysis.forEach((cb) => cb({ pass: "link_score", label: "Link Score", done: 0, total: 100, ...p })),
    analysed: [],
    notice: (n) => onNotice.forEach((cb) => cb({ id: nextId - 1, kind: "host_paused", host: null, ...n })),
    finish: (f) =>
      onFinished.forEach((cb) => cb({ id: nextId - 1, path: "/crawls/example.com.spiderlens", report: null, error: null, ...f })),
  };
  const resultsApi = fakeResultsTransport(urls);
  const compareApi = fakeCompareTransport(resultsApi.results.progress);
  const { api: integrationsApi, integrations } = fakeIntegrationsTransport();
  const aeoApi = fakeAeoTransport();
  const { files, api: filesApi } = fakeFilesTransport(crawls, () => nextId++);
  const agentsApi = fakeAgentsTransport(
    () => resultsApi.results.urls,
    () => resultsApi.results.features,
  );
  const { schedules, api: schedulesApi } = fakeSchedulesTransport((name) => saved.get(name)?.config ?? null);
  return {
    ...resultsApi,
    ...agentsApi,
    ...compareApi,
    ...fakeCustomTransport(),
    ...integrationsApi,
    integrations,
    ...aeoApi,
    ...schedulesApi,
    schedules,

    ...fakeAuditTransport(),
    ...fakeSpellingTransport(),
    saved,
    crawls,
    picks,
    sitemaps,
    pickFolder: async () => picks.folder ?? (await agentsApi.pickFolder()),
    sitemapFiles: async () => structuredClone(sitemaps.files),
    sitemapPreview: async (_path, options, folder) => {
      sitemaps.previews += 1;
      const r = fakeReport_(resultsApi.results.urls, options, folder ? (sitemaps.existing.get(folder) ?? []) : []);
      if (r.pages === 0) throw "No page matches these options.";
      return r;
    },
    sitemapGenerate: async (_path, options, folder, replace) => {
      if (sitemaps.fail) throw sitemaps.fail;
      const r = fakeReport_(resultsApi.results.urls, options, sitemaps.existing.get(folder) ?? []);
      if (r.pages === 0) throw "No page matches these options.";
      if (r.existing.length > 0 && !replace) throw `These files already exist: ${r.existing.join(", ")}`;
      sitemaps.generated.push({ folder, options, replace });
      sitemaps.existing.set(folder, [...new Set([...(sitemaps.existing.get(folder) ?? []), ...r.files.map((f) => f.name)])]);
      return { ...r, existing: [] };
    },
    pickCsvPath: async (name) => `/exports/${name}.csv`,
    pickVizPath: async (name, format) => `/exports/${name}.${format}`,
    buildInfo: async () => ({
      product: "SpiderLens",
      product_version: "0.1.0",
      engine_version: "0.1.0",
      build_id: "abc1234.7",
      commit: "abc1234def",
    }),
    defaults: async () => defaultConfig(),
    presets: async () => presets,
    validate: async (config, startUrl) => {
      const issues: Issue[] = [];
      let normalized: string | null = null;
      if (config.mode === "spider" && startUrl !== null) {
        const host = startUrl.replace(/^https?:\/\//, "").split(/[/:]/)[0];
        if (/^[a-z0-9-]+$/i.test(host) && host !== "localhost") {
          issues.push({ path: "input.start_url", severity: "error", code: "start_url_no_dot", message: `"${host}" is not a full web address.` });
        } else if (/^[a-z0-9.-]+(\/.*)?$/i.test(startUrl.replace(/^https?:\/\//, "")) && !startUrl.includes("!")) {
          normalized = startUrl.includes("://") ? startUrl : `https://${startUrl}/`;
        } else {
          issues.push({ path: "input.start_url", severity: "error", code: "start_url_invalid", message: "The start URL is not usable." });
        }
      }
      if (config.mode === "list" && listCount === 0) {
        issues.push({ path: "input.urls", severity: "error", code: "list_empty", message: "The list has no valid web addresses." });
      }
      if (config.identity.user_agent.preset !== "spiderlens") {
        issues.push({ path: "identity.user_agent.preset", severity: "warning", code: "ua_not_spiderlens", message: "The crawler will no longer identify itself as SpiderLens." });
      }
      if (config.robots.mode !== "respect") {
        issues.push({ path: "robots.mode", severity: "warning", code: "robots_ignored", message: "Pages disallowed by robots.txt will be crawled." });
      }
      config.links.position_rules.forEach((r, i) => {
        // Stand-in for the CSS parser: an unclosed bracket is invalid.
        if (r.selector.trim() === "" || r.selector.split("[").length !== r.selector.split("]").length)
          issues.push({ path: `links.position_rules[${i}].selector`, severity: "error", code: "selector_syntax", message: "Not a valid CSS selector." });
      });
      const r = config.retry;
      if (!(r.base_wait_secs >= 0.1 && r.base_wait_secs <= 60)) {
        issues.push({ path: "retry.base_wait_secs", severity: "error", code: "out_of_range", message: "The first wait must be between 0.1 and 60 seconds." });
      }
      if (!(r.host_pause_after >= 1 && r.host_pause_after <= 100)) {
        issues.push({ path: "retry.host_pause_after", severity: "error", code: "out_of_range", message: "Failures before a site is paused must be between 1 and 100." });
      }
      if (!(config.rendering.max_render_secs >= 1 && config.rendering.max_render_secs <= 60)) {
        issues.push({ path: "rendering.max_render_secs", severity: "error", code: "out_of_range", message: "The maximum render time must be between 1 and 60 seconds." });
      }
      config.segments.forEach((seg, i) => {
        if (!seg.name.trim()) issues.push({ path: `segments[${i}].name`, severity: "error", code: "segment_name_empty", message: "Give the segment a name." });
        seg.clauses.forEach((c, j) => {
          if (!c.value.trim()) issues.push({ path: `segments[${i}].clauses[${j}]`, severity: "error", code: "segment_value_empty", message: "Enter a value to compare with." });
        });
      });
      // Custom rules (spec 013): a few of the engine's checks, enough for the UI paths.
      config.custom.extractors.forEach((e, i) => {
        if (!e.expr.trim()) issues.push({ path: `custom.extractors[${i}].expr`, severity: "error", code: "custom_expr_empty", message: "Enter what to extract." });
        if (config.custom.extractors.findIndex((x) => x.name === e.name) !== i)
          issues.push({ path: `custom.extractors[${i}].name`, severity: "error", code: "custom_name_taken", message: `Another extractor is already called "${e.name}".` });
      });
      config.custom.searches.forEach((x, i) => {
        if (!x.term) issues.push({ path: `custom.searches[${i}].term`, severity: "error", code: "custom_term_empty", message: "Enter what to search for." });
      });
      const s = config.speed;
      if (s.max_concurrent > 8 || s.max_per_host > 4 || s.max_urls_per_sec === null || s.max_urls_per_sec > 5) {
        issues.push({ path: "speed", severity: "warning", code: "faster_than_polite", message: "Faster settings are intended for sites you control." });
      }
      return {
        issues,
        speed_preset: same(s, polite) ? "polite" : same(s, fast) ? "fast" : "custom",
        normalized_start_url: normalized,
      };
    },
    testPatterns: async (_i, _e, samples) =>
      samples.filter(Boolean).map((input) => ({ input, url: input, decision: { decision: "crawl", include: null }, pattern: null, error: null })),
    scopeTest: async (config, _start, urls) =>
      urls
        .filter((u) => u.trim())
        .map((input) => {
          const excluded = config.scope.exclude.find((p) => input.includes(p));
          return {
            input,
            url: input,
            would_crawl: !excluded,
            reason: excluded ? "excluded" : null,
            label: excluded ? "Excluded by rule" : null,
            detail: excluded ?? null,
            robots_checked: false,
            segment: (() => {
              try {
                return fakeSegmentOf(config.segments, { id: 0, url: input, status: null, title: null, depth: 0, internal: true })?.name ?? null;
              } catch {
                return null;
              }
            })(),
          };
        }),
    parseListText: async (text) => {
      const lines = text.split("\n").filter((l) => l.trim());
      const unique = new Set(lines.filter((l) => !l.includes("!")));
      listCount = unique.size;
      return {
        urls: unique.size,
        duplicates: lines.filter((l) => !l.includes("!")).length - unique.size,
        invalid_count: lines.filter((l) => l.includes("!")).length,
        invalid_sample: lines
          .map((text, i) => ({ line: i + 1, text, reason: "not a valid web address" }))
          .filter((l) => l.text.includes("!")),
        notices: [],
        first_urls: [...unique].slice(0, 20),
      };
    },
    parseListFile: async () => {
      throw new Error("not in tests");
    },
    onListProgress: async () => () => {},
    pickListFile: async () => null,
    pickSitemapFile: async () => picks.sitemapFile,
    pickProfileFile: async () => null,
    pickExportPath: async () => null,
    profiles: async () => [...saved.keys()].map((name) => ({ name, is_default: false, error: null })),
    loadProfile: async (name) => structuredClone(saved.get(name)!),
    saveProfile: async (name, config, startUrl, overwrite) => {
      if (saved.has(name) && !overwrite) return { result: "name_taken" };
      saved.set(name, { name, config: structuredClone(config), start_url: startUrl, ignored: [] });
      return { result: "saved" };
    },
    renameProfile: async () => {},
    duplicateProfile: async () => {},
    deleteProfile: async (name) => {
      saved.delete(name);
    },
    setDefaultProfile: async () => {},
    exportProfile: async () => {},
    importProfile: async () => {
      throw new Error("not in tests");
    },
    crawlStart: async (config, startUrl) => {
      crawls.started.push({ config, startUrl });
      return { id: nextId++, path: "/crawls/example.com.spiderlens" };
    },
    crawlPause: async (id) => void crawls.controls.push(`pause ${id}`),
    crawlStop: async (id) => void crawls.controls.push(`stop ${id}`),
    onCrawlProgress: (cb) => subscribe(onProgress, cb),
    onCrawlWarning: (cb) => subscribe(onWarning, cb),
    onPassProgress: (cb) => subscribe(onAnalysis, cb),
    passesRun: async (path) => {
      crawls.analysed.push(path);
      return { ran: [], skipped: [], saved: true };
    },
    onCrawlFinished: (cb) => subscribe(onFinished, cb),
    onCrawlNotice: (cb) => subscribe(onNotice, cb),
    crawlResumeHost: async (id, host) => void crawls.resumedHosts.push(`${id} ${host}`),
    crawlApplySettings: async (id, speed, retry) => void crawls.applied.push({ id, speed, retry }),
    retryPreview: async () => structuredClone(crawls.preview),
    retryStart: async (path) => {
      crawls.retried.push(path);
      return { id: nextId++, path };
    },
    crawlLog: async () => [],
    // Crawl files (spec 006), including pickCrawlFile, crawlResume and crawlSummary.
    ...filesApi,
    files,
  };
}

// ---------------------------------------------------------------------------
// Crawl files (spec 006): recent list, unsaved crawls, open, Save As, resume
// prompts and the window events. Everything a test needs is on `files`.
// ---------------------------------------------------------------------------

/** A crawl file's information, as the engine would report it. */
export function fakeCrawlInfo(path: string, patch: Partial<CrawlInfo> = {}): CrawlInfo {
  const name = (path.split(/[\\/]/).pop() ?? path).replace(/\.spiderlens$/i, "");
  return {
    path,
    name,
    size_bytes: 48 * 1024 * 1024,
    format_version: 2,
    status: "completed",
    pause_reason: null,
    recovered: false,
    mode: "spider",
    start_url: "https://example.com/",
    config: defaultConfig(),
    started_at: "2026-10-12T09:00:00Z",
    ended_at: "2026-10-12T09:42:00Z",
    counts: { discovered: 120, crawled: 100, failed: 2, not_crawled: 18, queued: 0 },
    versions: [{ kind: "created", app_version: "0.6.0", os: "macOS 15.1", at: "2026-10-12T09:00:00Z" }],
    events: [
      { at: "2026-10-12T09:00:00Z", kind: "started", app_version: "0.6.0", os: "macOS 15.1", detail: null },
      { at: "2026-10-12T09:42:00Z", kind: "completed", app_version: "0.6.0", os: "macOS 15.1", detail: null },
    ],
    access: { kind: "writable" },
    missing_secrets: [],
    unsaved: false,
    ...patch,
  };
}

/** A recent-list entry. */
export function fakeRecent(path: string, patch: Partial<RecentEntry> = {}): RecentEntry {
  const name = (path.split(/[\\/]/).pop() ?? path).replace(/\.spiderlens$/i, "");
  return {
    path,
    name,
    start_url: `https://${name}/`,
    urls: 120,
    status: "completed",
    recovered: false,
    last_used: "2026-10-12T09:42:00Z",
    available: true,
    unsaved: false,
    ...patch,
  };
}

export function fakeLocalError(kind: LocalError["kind"] = "disk_full", patch: Partial<LocalError> = {}): LocalError {
  return { kind, message: "No space left on device", path: "/crawls/example.com.spiderlens", free_bytes: 0, ...patch };
}

export interface FakeFiles {
  /** Most recent first. Tests may assign to it. */
  recent: RecentEntry[];
  infos: Map<string, CrawlInfo>;
  unsaved: { entry: RecentEntry; size: number }[];
  storage: StorageStatus;
  /** Paths whose open fails as damaged or not a crawl file. */
  damaged: Map<string, OpenFailure>;
  /** What the file pickers return. */
  pickPath: string | null;
  /** Save dialog answer: a path, null for Cancel, undefined for /saved/<name>.spiderlens. */
  saveDest: string | null | undefined;
  pending: string[];
  opened: string[];
  salvaged: string[];
  savedAs: { path: string; dest: string }[];
  resumes: { path: string; options: ResumeOptions | undefined }[];
  removed: string[];
  deleted: string[];
  seedRecent(...entries: RecentEntry[]): void;
  seedInfo(path: string, patch?: Partial<CrawlInfo>): CrawlInfo;
  seedUnsaved(path: string, size: number, patch?: Partial<RecentEntry>): void;
  openFile(path: string): void;
  drop(paths: string[]): void;
  moved(m: Partial<CrawlMoved> & { to: string }): void;
  quitting(pausing?: boolean): void;
  recovered(entries: RecentEntry[]): void;
  // Crawl data management (spec 019).
  /** File sizes by path; default 10 MB. */
  sizes: Map<string, number>;
  /** Stored HTML by path; default none. */
  html: Map<string, HtmlSize>;
  backups: Map<string, CrawlBackup[]>;
  /** Paths whose drive has no trash. */
  noTrash: Set<string>;
  /** Paths in use elsewhere, with the reason shown. */
  inUse: Map<string, string>;
  /** Paths without room to compact. */
  noCompactRoom: Set<string>;
  running: string | null;
  trashed: string[];
  deletedPermanently: string[];
  purged: { path: string; compact: boolean }[];
  revealed: string[];
  prefs: StoragePrefs;
  history: CleanupRecord[];
  leftoverBytes: number;
  cleanupDone(r: Partial<CleanupReport>): void;
  crawlsChanged(): void;
}

type FilesApi = Pick<
  Transport,
  | "pickCrawlFile"
  | "crawlResume"
  | "crawlSummary"
  | "crawlOpen"
  | "crawlInfo"
  | "crawlSaveAs"
  | "pickSavePath"
  | "recentList"
  | "recentRemove"
  | "recentLocate"
  | "unsavedList"
  | "unsavedDelete"
  | "crawlSalvage"
  | "storageStatus"
  | "pendingOpen"
  | "onOpenFile"
  | "onFileDrop"
  | "onCrawlMoved"
  | "onAppQuitting"
  | "onCrawlsRecovered"
  | "manageList"
  | "htmlSize"
  | "revealPath"
  | "crawlsDelete"
  | "crawlsDeletePermanently"
  | "purgeHtmlCheck"
  | "purgeHtml"
  | "storagePrefs"
  | "setStoragePrefs"
  | "cleanupHistory"
  | "onCleanupDone"
  | "onCrawlsChanged"
>;

const MB = 1024 ** 2;

/** Storage settings as a fresh install has them. */
export function defaultStoragePrefs(): StoragePrefs {
  return { version: 1, cleanup_days: 30, reserve_bytes: 1024 * MB, first_cleanup_announced: null, last_cleanup: null };
}

function fakeFilesTransport(crawls: FakeCrawls, nextCrawlId: () => number) {
  const openFile: Listeners<string> = new Set();
  const drop: Listeners<string[]> = new Set();
  const moved: Listeners<CrawlMoved> = new Set();
  const quitting: Listeners<{ pausing: boolean }> = new Set();
  const recovered: Listeners<RecentEntry[]> = new Set();
  const cleanupDone: Listeners<CleanupReport> = new Set();
  const crawlsChanged: Listeners<void> = new Set();
  const on = async <T>(set: Listeners<T>, cb: (v: T) => void) => {
    set.add(cb);
    return () => void set.delete(cb);
  };
  // Unseeded files take what the recent list says about them.
  const info = (path: string) => {
    const seeded = files.infos.get(path);
    if (seeded) return seeded;
    const r = files.recent.find((e) => e.path === path);
    return fakeCrawlInfo(path, r ? { name: r.name, start_url: r.start_url, status: r.status, recovered: r.recovered, unsaved: r.unsaved } : {});
  };
  const touch = (path: string) => {
    const i = info(path);
    const old = files.recent.find((r) => r.path === path);
    const entry = old
      ? { ...old, available: true, last_used: new Date().toISOString() }
      : fakeRecent(path, { name: i.name, start_url: i.start_url, status: i.status, recovered: i.recovered, unsaved: i.unsaved, last_used: new Date().toISOString() });
    files.recent = [entry, ...files.recent.filter((r) => r.path !== path)];
  };

  const files: FakeFiles = {
    recent: [],
    infos: new Map(),
    unsaved: [],
    storage: { unsaved_dir: "/data/SpiderLens/Unsaved", free_bytes: 200 * 1024 ** 3, reserve_bytes: 2 * 1024 ** 3, low: false },
    damaged: new Map(),
    pickPath: null,
    saveDest: undefined,
    pending: [],
    opened: [],
    salvaged: [],
    savedAs: [],
    resumes: [],
    removed: [],
    deleted: [],
    seedRecent: (...entries) => void (files.recent = [...files.recent, ...entries]),
    seedInfo: (path, patch) => {
      const i = fakeCrawlInfo(path, patch);
      files.infos.set(path, i);
      return i;
    },
    seedUnsaved: (path, size, patch) => {
      const entry = fakeRecent(path, { unsaved: true, ...patch });
      files.unsaved.push({ entry, size });
      if (!files.recent.some((r) => r.path === path)) files.recent.push(entry);
    },
    openFile: (path) => openFile.forEach((cb) => cb(path)),
    drop: (paths) => drop.forEach((cb) => cb(paths)),
    moved: (m) => moved.forEach((cb) => cb({ id: 0, from: "", ...m })),
    quitting: (pausing = true) => quitting.forEach((cb) => cb({ pausing })),
    recovered: (entries) => recovered.forEach((cb) => cb(entries)),
    sizes: new Map(),
    html: new Map(),
    backups: new Map(),
    noTrash: new Set(),
    inUse: new Map(),
    noCompactRoom: new Set(),
    running: null,
    trashed: [],
    deletedPermanently: [],
    purged: [],
    revealed: [],
    prefs: defaultStoragePrefs(),
    history: [],
    leftoverBytes: 0,
    cleanupDone: (r) => cleanupDone.forEach((cb) => cb({ removed: [], leftovers: 0, freed_bytes: 0, announced: [], ...r })),
    crawlsChanged: () => crawlsChanged.forEach((cb) => cb()),
  };

  const sizeOf = (path: string) => files.sizes.get(path) ?? files.unsaved.find((u) => u.entry.path === path)?.size ?? 10 * MB;
  const managed = (): ManagedCrawl[] => {
    const paths = [...files.unsaved.map((u) => u.entry.path), ...files.recent.map((r) => r.path)].filter((p, i, a) => a.indexOf(p) === i);
    return paths.map((path) => {
      const r = files.recent.find((e) => e.path === path) ?? files.unsaved.find((u) => u.entry.path === path)!.entry;
      return {
        path,
        name: r.name,
        start_url: r.start_url,
        status: files.running === path ? "running" : r.status,
        urls: r.urls,
        size_bytes: sizeOf(path),
        created: r.last_used,
        last_used: r.last_used,
        available: r.available,
        view_only: info(path).access.kind === "view_only",
        backups: files.backups.get(path) ?? [],
        unsaved: r.unsaved,
        running: files.running === path,
        in_recent: files.recent.some((e) => e.path === path),
      };
    });
  };
  const forget = (path: string) => {
    files.unsaved = files.unsaved.filter((u) => u.entry.path !== path);
    files.recent = files.recent.filter((r) => r.path !== path);
    for (const [k, b] of files.backups) files.backups.set(k, b.filter((x) => x.path !== path));
  };
  const remove = (paths: string[], permanently: boolean) => {
    const report = { deleted: [] as string[], skipped: [] as { path: string; reason: string }[], trash_unavailable: [] as string[], freed_bytes: 0 };
    for (const path of paths) {
      if (files.running === path) report.skipped.push({ path, reason: "Stop the crawl before deleting it." });
      else if (files.inUse.has(path)) report.skipped.push({ path, reason: files.inUse.get(path)! });
      else if (!permanently && files.noTrash.has(path)) report.trash_unavailable.push(path);
      else {
        const backup = [...files.backups.values()].flat().find((b) => b.path === path);
        report.freed_bytes += backup ? backup.size_bytes : sizeOf(path);
        report.deleted.push(path);
        (permanently ? files.deletedPermanently : files.trashed).push(path);
        files.history.unshift({ at: new Date().toISOString(), path, bytes: sizeOf(path), reason: permanently ? "manual_delete_permanent" : "manual_delete" });
        forget(path);
      }
    }
    return report;
  };

  const api: FilesApi = {
    pickCrawlFile: async () => files.pickPath,
    crawlResume: async (path, options) => {
      crawls.resumed.push(path);
      files.resumes.push({ path, options });
      return { id: nextCrawlId(), path };
    },
    crawlSummary: async (path) => fakeReport(info(path).status, 1, path),
    crawlOpen: async (path) => {
      const failure = files.damaged.get(path);
      if (failure) throw JSON.stringify(failure);
      files.opened.push(path);
      touch(path);
      return info(path);
    },
    crawlInfo: async (path) => info(path),
    crawlSaveAs: async (path, dest) => {
      files.savedAs.push({ path, dest });
      files.infos.set(dest, { ...info(path), path: dest, name: dest.split(/[\\/]/).pop()!.replace(/\.spiderlens$/i, ""), unsaved: false, access: { kind: "writable" } });
      files.recent = files.recent.map((r) => (r.path === path ? { ...r, path: dest, unsaved: false } : r));
      return dest;
    },
    pickSavePath: async (name) => (files.saveDest === undefined ? `/saved/${name}.spiderlens` : files.saveDest),
    recentList: async () => structuredClone(files.recent),
    recentRemove: async (path) => {
      files.removed.push(path);
      files.recent = files.recent.filter((r) => r.path !== path);
    },
    recentLocate: async (oldPath, newPath) => {
      const old = files.recent.find((r) => r.path === oldPath) ?? fakeRecent(newPath);
      const entry = { ...old, path: newPath, available: true };
      files.recent = files.recent.map((r) => (r.path === oldPath ? entry : r));
      return entry;
    },
    unsavedList: async () => ({
      entries: files.unsaved.map((u) => u.entry),
      total_bytes: files.unsaved.reduce((n, u) => n + u.size, 0),
      size_bytes: Object.fromEntries(files.unsaved.map((u) => [u.entry.path, u.size])),
    }),
    unsavedDelete: async (path) => {
      files.deleted.push(path);
      files.trashed.push(path);
      files.unsaved = files.unsaved.filter((u) => u.entry.path !== path);
      files.recent = files.recent.filter((r) => r.path !== path);
    },
    crawlSalvage: async (path) => {
      files.salvaged.push(path);
      const to = `/data/SpiderLens/Unsaved/${info(path).name}-recovered.spiderlens`;
      return files.seedInfo(to, { recovered: true, unsaved: true, status: "paused" });
    },
    storageStatus: async () => ({ ...files.storage }),
    pendingOpen: async () => {
      const p = files.pending;
      files.pending = [];
      return p;
    },
    onOpenFile: (cb) => on(openFile, cb),
    onFileDrop: (cb) => on(drop, cb),
    onCrawlMoved: (cb) => on(moved, cb),
    onAppQuitting: (cb) => on(quitting, cb),
    onCrawlsRecovered: (cb) => on(recovered, cb),
    manageList: async () => {
      const crawls = managed();
      return {
        crawls,
        other_files: [],
        totals: {
          all: crawls.reduce((n, c) => n + c.size_bytes + c.backups.reduce((m, b) => m + b.size_bytes, 0), 0),
          unsaved: crawls.filter((c) => c.unsaved).reduce((n, c) => n + c.size_bytes, 0),
          leftovers: files.leftoverBytes,
        },
        free_bytes: files.storage.free_bytes,
        unsaved_dir: files.storage.unsaved_dir,
      };
    },
    htmlSize: async (path) => files.html.get(path) ?? { bytes: 0, pages: 0 },
    revealPath: async (path) => void files.revealed.push(path),
    crawlsDelete: async (paths) => remove(paths, false),
    crawlsDeletePermanently: async (paths) => remove(paths, true),
    purgeHtmlCheck: async (path) => {
      const h = files.html.get(path) ?? { bytes: 0, pages: 0 };
      const size = sizeOf(path);
      return {
        html_bytes: h.bytes,
        html_pages: h.pages,
        file_bytes: size,
        new_size: size - h.bytes,
        free_bytes: files.noCompactRoom.has(path) ? 1 * MB : files.storage.free_bytes,
        can_compact: !files.noCompactRoom.has(path),
      };
    },
    purgeHtml: async (path, compact) => {
      if (files.running === path) throw "Pause or stop the crawl before removing its stored HTML.";
      const h = files.html.get(path) ?? { bytes: 0, pages: 0 };
      files.purged.push({ path, compact });
      files.html.set(path, { bytes: 0, pages: 0 });
      const freed = compact ? h.bytes : 0;
      files.sizes.set(path, sizeOf(path) - freed);
      return { html_pages: h.pages, html_bytes: h.bytes, freed_bytes: freed, compacted: compact };
    },
    storagePrefs: async () => ({ ...files.prefs }),
    setStoragePrefs: async (prefs) => {
      const issues = [];
      if (prefs.reserve_bytes < 256 * MB || prefs.reserve_bytes > 50 * 1024 * MB)
        issues.push({ path: "storage.reserve_bytes", code: "reserve_range", message: "The safety reserve must be between 256 MB and 50 GB.", severity: "error" });
      if (prefs.cleanup_days !== null && ![7, 14, 30, 90].includes(prefs.cleanup_days))
        issues.push({ path: "storage.cleanup_days", code: "cleanup_days", message: "Choose 7, 14, 30 or 90 days, or turn automatic cleanup off.", severity: "error" });
      if (issues.length) throw JSON.stringify(issues);
      files.prefs = { ...files.prefs, cleanup_days: prefs.cleanup_days, reserve_bytes: prefs.reserve_bytes };
      files.storage = { ...files.storage, reserve_bytes: prefs.reserve_bytes };
      return { ...files.prefs };
    },
    cleanupHistory: async () => structuredClone(files.history),
    onCleanupDone: (cb) => on(cleanupDone, cb),
    onCrawlsChanged: (cb) => on(crawlsChanged, () => cb()),
  };
  return { files, api };
}
