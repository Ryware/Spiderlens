// In-memory Transport for UI tests. Validation here is a small stand-in; the
// real rules are tested in Rust (spiderlens-config).

import type { CrawlConfig } from "../generated/CrawlConfig";
import type { Issue } from "../generated/Issue";
import type { RetrySettings } from "../generated/RetrySettings";
import type { SpeedSettings } from "../generated/SpeedSettings";
import { fakeResultsTransport, fakeUrls, type FakeResults, type FakeUrl } from "./fakeResults";
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
  RetryPreview,
  RecentEntry,
  ResumeOptions,
  StorageStatus,
  Transport,
} from "../transport";

const polite: SpeedSettings = { max_concurrent: 8, max_per_host: 4, max_urls_per_sec: 5, adaptive: true };
const fast: SpeedSettings = { max_concurrent: 64, max_per_host: 32, max_urls_per_sec: null, adaptive: true };

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
      use_sitemaps: false,
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
    extraction: { store_html: true, pdf: { crawl: false, follow_links: true, max_mb: 20, time_limit_secs: 5 } },
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
  notice(n: Partial<CrawlNotice> & { message: string }): void;
  finish(f: Partial<CrawlFinished>): void;
}

export function fakeTransport(
  urls: FakeUrl[] = fakeUrls(30),
): Transport & { saved: Map<string, LoadedProfile>; crawls: FakeCrawls; results: FakeResults; files: FakeFiles } {
  const saved = new Map<string, LoadedProfile>();
  let listCount = 0;
  let nextId = 1;
  const onProgress: Listeners<CrawlProgress> = new Set();
  const onWarning: Listeners<{ id: number; message: string }> = new Set();
  const onFinished: Listeners<CrawlFinished> = new Set();
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
          ...p,
        }),
      ),
    warn: (message) => onWarning.forEach((cb) => cb({ id: nextId - 1, message })),
    notice: (n) => onNotice.forEach((cb) => cb({ id: nextId - 1, kind: "host_paused", host: null, ...n })),
    finish: (f) =>
      onFinished.forEach((cb) => cb({ id: nextId - 1, path: "/crawls/example.com.spiderlens", report: null, error: null, ...f })),
  };
  const resultsApi = fakeResultsTransport(urls);
  const { files, api: filesApi } = fakeFilesTransport(crawls, () => nextId++);
  return {
    ...resultsApi,
    saved,
    crawls,
    pickCsvPath: async (name) => `/exports/${name}.csv`,
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
      const r = config.retry;
      if (!(r.base_wait_secs >= 0.1 && r.base_wait_secs <= 60)) {
        issues.push({ path: "retry.base_wait_secs", severity: "error", code: "out_of_range", message: "The first wait must be between 0.1 and 60 seconds." });
      }
      if (!(r.host_pause_after >= 1 && r.host_pause_after <= 100)) {
        issues.push({ path: "retry.host_pause_after", severity: "error", code: "out_of_range", message: "Failures before a site is paused must be between 1 and 100." });
      }
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
>;

function fakeFilesTransport(crawls: FakeCrawls, nextCrawlId: () => number) {
  const openFile: Listeners<string> = new Set();
  const drop: Listeners<string[]> = new Set();
  const moved: Listeners<CrawlMoved> = new Set();
  const quitting: Listeners<{ pausing: boolean }> = new Set();
  const recovered: Listeners<RecentEntry[]> = new Set();
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
  };
  return { files, api };
}
