// The only module that knows how the UI talks to the engine. The desktop app
// uses Tauri commands; a future web version replaces this file with HTTP calls
// and leaves every component untouched (constitution principle III).

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { CrawlConfig } from "./generated/CrawlConfig";
import type { Features } from "./generated/Features";
import type { FilterCounts } from "./generated/FilterCounts";
import type { ListKind } from "./generated/ListKind";
import type { ListPage } from "./generated/ListPage";
import type { Refresh } from "./generated/Refresh";
import type { RowPage } from "./generated/RowPage";
import type { TabDef } from "./generated/TabDef";
import type { UrlDetail } from "./generated/UrlDetail";
import type { UrlScope } from "./generated/UrlScope";
import type { ViewQuery } from "./generated/ViewQuery";
import type { InvalidLine } from "./generated/InvalidLine";
import type { Issue } from "./generated/Issue";
import type { PatternTest } from "./generated/PatternTest";
import type { ProfileEntry } from "./generated/ProfileEntry";
import type { SpeedPreset } from "./generated/SpeedPreset";
import type { SpeedSettings } from "./generated/SpeedSettings";
import type { UaPreset } from "./generated/UaPreset";
import type { ActivityEntry } from "./generated/ActivityEntry";
import type { AssistantSettings } from "./generated/AssistantSettings";
import type { AssistantSnippet } from "./generated/AssistantSnippet";
import type { PageAgentFindings } from "./generated/PageAgentFindings";

export interface BuildInfo {
  product: string;
  product_version: string;
  engine_version: string;
  build_id: string;
  commit: string | null;
}

export interface UaPresetInfo {
  preset: UaPreset;
  label: string;
  user_agent: string | null;
  robots_token: string | null;
}

export interface Presets {
  user_agents: UaPresetInfo[];
  polite: SpeedSettings;
  fast: SpeedSettings;
}

export interface Validation {
  issues: Issue[];
  speed_preset: SpeedPreset;
  normalized_start_url: string | null;
}

export interface ListSummary {
  urls: number;
  duplicates: number;
  invalid_count: number;
  invalid_sample: InvalidLine[];
  notices: string[];
  first_urls: string[];
}

export interface LoadedProfile {
  name: string;
  config: CrawlConfig;
  start_url: string | null;
  ignored: string[];
}

/** The scope tester's answer for one URL (spec 003 FR-016). */
export interface ScopeVerdict {
  input: string;
  url: string | null;
  would_crawl: boolean;
  reason: string | null;
  /** The reason as the app words it everywhere else. */
  label: string | null;
  detail: string | null;
  robots_checked: boolean;
}

export type SaveResult = { result: "saved" } | { result: "name_taken" };

/** A crawl the engine accepted. Its file is saved as the crawl runs. */
export interface CrawlStarted {
  id: number;
  path: string;
}

export interface CrawlProgress {
  id: number;
  discovered: number;
  crawled: number;
  in_flight: number;
  queued: number;
  not_crawled: number;
  errors: number;
  /** Requests finished per second over the last 5 seconds (spec 008). */
  urls_per_sec: number;
  avg_urls_per_sec: number;
  /** Seconds left for the URLs queued now; rises while discovery outpaces crawling. */
  eta_secs: number | null;
  /** Time spent crawling, pauses excluded, across every run of this file (spec 007). */
  active_ms: number;
}

export type CrawlStatus = "running" | "paused" | "stopped" | "completed";

/** contracts/crawl-summary.schema.json (spec 003). */
export interface CrawlSummary {
  status: CrawlStatus;
  discovered: number;
  crawled: number;
  not_crawled: number;
  queued: number;
  by_reason: Record<string, number>;
  external_checked: number;
  external_blocked: number;
  url_limit_reached: boolean;
  hosts: { host: string; robots: "ok" | "not_found" | "forbidden" | "unavailable" | "ignored" | "pending"; crawl_delay_secs: number | null; error: string | null }[];
  sitemaps: { url: string; urls: number; error: string | null }[];
  warnings: string[];
  /** Crawled URLs that failed without a response, per error class (spec 007). */
  failed_by_class?: Record<string, number>;
  active_ms?: number;
}

export interface CrawlReport {
  path: string;
  summary: CrawlSummary;
  /** Not-crawled reasons with the engine's labels, in FR-038 order. */
  reasons: { reason: string; label: string; count: number }[];
  /** Crawled URLs that got no response, per error class, with labels. */
  failures?: { class: string; label: string; count: number }[];
}

export interface CrawlFinished {
  id: number;
  path: string;
  report: CrawlReport | null;
  error: string | null;
  /** Set when a disk or write problem paused the crawl (spec 006 FR-029). */
  local?: LocalError | null;
}

// Crawl files (spec 006, specs/006-crawl-storage/contracts/desktop-commands.md).

export type LocalErrorKind = "disk_low" | "disk_full" | "drive_gone" | "permission" | "other";

/** A disk or write problem that paused the crawl; never a website error. */
export interface LocalError {
  kind: LocalErrorKind;
  message: string;
  path: string;
  free_bytes: number | null;
}

export interface LockNote {
  app_version: string;
  pid: number;
  host: string;
  since: string;
}

/** What may be done with a crawl file right now. */
export type Access =
  | { kind: "writable" }
  | { kind: "in_use"; by: LockNote }
  | { kind: "read_only_media" }
  | { kind: "newer_format"; needs: string }
  | { kind: "needs_upgrade"; from: number }
  | { kind: "view_only"; reason: string };

export interface VersionRecord {
  kind: "created" | "resumed" | "upgraded";
  app_version: string;
  os: string;
  at: string;
}

export interface CrawlEventRow {
  at: string;
  kind: string;
  app_version: string;
  os: string;
  detail: string | null;
}

export interface CrawlInfo {
  path: string;
  name: string;
  size_bytes: number;
  format_version: number;
  status: CrawlStatus;
  pause_reason: string | null;
  recovered: boolean;
  mode: "spider" | "list";
  start_url: string | null;
  /** The stored configuration; null when the file has none it can read. */
  config: CrawlConfig | null;
  started_at: string | null;
  ended_at: string | null;
  counts: { discovered: number; crawled: number; failed: number; not_crawled: number; queued: number };
  versions: VersionRecord[];
  events: CrawlEventRow[];
  access: Access;
  /** Header names whose secret values must be entered again to resume. */
  missing_secrets: string[];
  /** Inside the app's folder for crawls not yet saved to a chosen place. */
  unsaved: boolean;
}

export interface RecentEntry {
  path: string;
  name: string;
  start_url: string | null;
  urls: number;
  status: CrawlStatus;
  recovered: boolean;
  last_used: string;
  available: boolean;
  unsaved: boolean;
}

export interface UnsavedCrawls {
  entries: RecentEntry[];
  total_bytes: number;
  size_bytes: Record<string, number>;
}

export interface StorageStatus {
  unsaved_dir: string;
  free_bytes: number | null;
  reserve_bytes: number;
  low: boolean;
}

/** Speed and URL-limit changes allowed on resume (FR-011). */
export interface ResumeChanges {
  speed?: SpeedSettings;
  max_urls?: number | null;
}

export interface ResumeOptions {
  changes?: ResumeChanges;
  secrets?: Record<string, string>;
  upgrade?: "keep_copy" | "no_copy";
}

export interface CrawlMoved {
  id: number;
  from: string;
  to: string;
}

/** A damaged or not-a-crawl file (FR-038). */
export interface OpenFailure {
  message: string;
  /** Part of it is readable: offer to recover what can be read. */
  salvageable: boolean;
}

/** A view's full order is built; rows can now be read at any position. */
export interface ViewReady {
  view: number;
  total: number;
}

export interface ExportProgress {
  job: number;
  rows_written: number;
  total: number;
  state: "running" | "done" | "cancelled" | "failed";
  error: string | null;
  path: string;
}

/** Settings › AI assistants (spec 016): the saved switches, this app's
 * program file and the configuration snippets that start it as an MCP server. */
export interface AssistantsView {
  settings: AssistantSettings;
  executable: string;
  snippets: AssistantSnippet[];
}

/** What `<app> mcp --self-test` printed: the tools an assistant would see. */
export interface McpSelfTest {
  server: string;
  version: string;
  protocol: string;
  read_access: boolean;
  crawl_access: boolean;
  tools: string[];
}

/** Positions [from, to) in a view: a selection. */
export type Range = [number, number];

export interface Transport {
  buildInfo(): Promise<BuildInfo>;
  defaults(builtin: boolean): Promise<CrawlConfig>;
  presets(): Promise<Presets>;
  validate(config: CrawlConfig, startUrl: string | null): Promise<Validation>;
  testPatterns(include: string[], exclude: string[], samples: string[]): Promise<PatternTest[]>;
  scopeTest(config: CrawlConfig, startUrl: string, urls: string[], fetchRobots: boolean): Promise<ScopeVerdict[]>;
  parseListText(text: string): Promise<ListSummary>;
  parseListFile(path: string): Promise<ListSummary>;
  onListProgress(cb: (lines: number) => void): Promise<() => void>;
  pickListFile(): Promise<string | null>;
  pickProfileFile(): Promise<string | null>;
  pickExportPath(name: string): Promise<string | null>;
  profiles(): Promise<ProfileEntry[]>;
  loadProfile(name: string): Promise<LoadedProfile>;
  saveProfile(name: string, config: CrawlConfig, startUrl: string | null, overwrite: boolean): Promise<SaveResult>;
  renameProfile(from: string, to: string): Promise<void>;
  duplicateProfile(from: string, to: string): Promise<void>;
  deleteProfile(name: string): Promise<void>;
  setDefaultProfile(name: string | null): Promise<void>;
  exportProfile(name: string, path: string): Promise<void>;
  importProfile(path: string): Promise<LoadedProfile>;
  crawlStart(config: CrawlConfig, startUrl: string | null): Promise<CrawlStarted>;
  /** Rejects with "needs_upgrade:", "needs_secrets:", "read_only:" or "in_use:" plus a message. */
  crawlResume(path: string, options?: ResumeOptions): Promise<CrawlStarted>;
  crawlPause(id: number): Promise<void>;
  crawlStop(id: number): Promise<void>;
  crawlSummary(path: string): Promise<CrawlReport>;
  onCrawlProgress(cb: (p: CrawlProgress) => void): Promise<() => void>;
  onCrawlWarning(cb: (w: { id: number; message: string }) => void): Promise<() => void>;
  onCrawlFinished(cb: (f: CrawlFinished) => void): Promise<() => void>;
  pickCrawlFile(): Promise<string | null>;
  pickCsvPath(name: string): Promise<string | null>;
  // Crawl files (spec 006).
  /** Opens a crawl file for viewing: recovers it if needed and adds it to the recent list. Rejects with an OpenFailure JSON string when damaged. */
  crawlOpen(path: string): Promise<CrawlInfo>;
  crawlInfo(path: string): Promise<CrawlInfo>;
  /** Saves the crawl to dest; a running crawl keeps running in the new file. Returns the new path. */
  crawlSaveAs(path: string, dest: string): Promise<string>;
  pickSavePath(name: string): Promise<string | null>;
  recentList(): Promise<RecentEntry[]>;
  recentRemove(path: string): Promise<void>;
  recentLocate(oldPath: string, newPath: string): Promise<RecentEntry>;
  unsavedList(): Promise<UnsavedCrawls>;
  unsavedDelete(path: string): Promise<void>;
  /** Copies what can be read from a damaged file into a new unsaved crawl. */
  crawlSalvage(path: string): Promise<CrawlInfo>;
  storageStatus(): Promise<StorageStatus>;
  /** Files the OS asked the app to open before the window was ready. */
  pendingOpen(): Promise<string[]>;
  onOpenFile(cb: (path: string) => void): Promise<() => void>;
  onFileDrop(cb: (paths: string[]) => void): Promise<() => void>;
  onCrawlMoved(cb: (m: CrawlMoved) => void): Promise<() => void>;
  onAppQuitting(cb: (q: { pausing: boolean }) => void): Promise<() => void>;
  onCrawlsRecovered(cb: (entries: RecentEntry[]) => void): Promise<() => void>;
  // Results (spec 007, contracts/desktop-commands.md).
  resultsTabs(): Promise<TabDef[]>;
  resultsOpen(path: string): Promise<{ features: Features }>;
  viewOpen(path: string, query: ViewQuery, count: number): Promise<RowPage>;
  viewRows(path: string, view: number, from: number, count: number): Promise<RowPage>;
  viewLocate(path: string, view: number, ids: number[]): Promise<(number | null)[]>;
  viewRefresh(path: string, view: number, anchors: number[]): Promise<Refresh | null>;
  viewCounts(path: string, tab: string, scope: UrlScope): Promise<FilterCounts>;
  viewClose(path: string, view: number): Promise<void>;
  urlDetail(path: string, id: number): Promise<UrlDetail>;
  urlList(path: string, id: number, kind: ListKind, from: number, count: number): Promise<ListPage>;
  /** Puts the rows on the clipboard as TSV with a header and returns that text. */
  copyRows(path: string, view: number, ranges: Range[], columns: string[]): Promise<string>;
  exportStart(path: string, query: ViewQuery, columns: string[], ranges: Range[] | null, dest: string): Promise<number>;
  exportCancel(job: number): Promise<void>;
  onViewReady(cb: (r: ViewReady) => void): Promise<() => void>;
  onExportProgress(cb: (p: ExportProgress) => void): Promise<() => void>;
  // AI agents (spec 016, contracts/desktop-commands.md).
  agentsPage(path: string, id: number): Promise<PageAgentFindings>;
  assistantsGet(): Promise<AssistantsView>;
  /** Rejects with a sentence when a folder does not exist. */
  assistantsSet(settings: AssistantSettings): Promise<AssistantSettings>;
  /** Rejects with a sentence when the server does not start or answer. */
  assistantsTest(): Promise<McpSelfTest>;
  assistantsActivity(limit: number): Promise<ActivityEntry[]>;
  assistantsActivityClear(): Promise<void>;
  pickFolder(): Promise<string | null>;
}

const asPath = (p: string | string[] | null) => (Array.isArray(p) ? (p[0] ?? null) : p);

export const tauriTransport: Transport = {
  buildInfo: () => invoke("build_info"),
  defaults: (builtin) => invoke("config_defaults", { builtin }),
  presets: () => invoke("config_presets"),
  validate: (config, startUrl) => invoke("config_validate", { config, startUrl }),
  testPatterns: (include, exclude, samples) => invoke("patterns_test", { include, exclude, samples }),
  scopeTest: (config, startUrl, urls, fetchRobots) => invoke("scope_test", { config, startUrl, urls, fetchRobots }),
  parseListText: (text) => invoke("list_parse_text", { text }),
  parseListFile: (path) => invoke("list_parse_file", { path }),
  onListProgress: (cb) => listen<number>("list-progress", (e) => cb(e.payload)),
  pickListFile: async () =>
    asPath(await open({ filters: [{ name: "URL lists", extensions: ["txt", "csv", "tsv", "list"] }] })),
  pickProfileFile: async () =>
    asPath(await open({ filters: [{ name: "SpiderLens profile", extensions: ["json"] }] })),
  pickExportPath: (name) =>
    save({ defaultPath: `${name}.spiderlens-profile.json`, filters: [{ name: "SpiderLens profile", extensions: ["json"] }] }),
  profiles: () => invoke("profiles_list"),
  loadProfile: (name) => invoke("profile_load", { name }),
  saveProfile: (name, config, startUrl, overwrite) => invoke("profile_save", { name, config, startUrl, overwrite }),
  renameProfile: (from, to) => invoke("profile_rename", { from, to }),
  duplicateProfile: (from, to) => invoke("profile_duplicate", { from, to }),
  deleteProfile: (name) => invoke("profile_delete", { name }),
  setDefaultProfile: (name) => invoke("profile_set_default", { name }),
  exportProfile: (name, path) => invoke("profile_export", { name, path }),
  importProfile: (path) => invoke("profile_import", { path }),
  crawlStart: (config, startUrl) => invoke("crawl_start", { config, startUrl }),
  crawlResume: (path, options) => invoke("crawl_resume", { path, ...options }),
  crawlPause: (id) => invoke("crawl_pause", { id }),
  crawlStop: (id) => invoke("crawl_stop", { id }),
  crawlSummary: (path) => invoke("crawl_summary", { path }),
  onCrawlProgress: (cb) => listen<CrawlProgress>("crawl-progress", (e) => cb(e.payload)),
  onCrawlWarning: (cb) => listen<{ id: number; message: string }>("crawl-warning", (e) => cb(e.payload)),
  onCrawlFinished: (cb) => listen<CrawlFinished>("crawl-finished", (e) => cb(e.payload)),
  pickCrawlFile: async () => asPath(await open({ filters: [{ name: "SpiderLens crawl", extensions: ["spiderlens"] }] })),
  pickCsvPath: (name) => save({ defaultPath: `${name}.csv`, filters: [{ name: "CSV", extensions: ["csv"] }] }),
  crawlOpen: (path) => invoke("crawl_open", { path }),
  crawlInfo: (path) => invoke("crawl_info", { path }),
  crawlSaveAs: (path, dest) => invoke("crawl_save_as", { path, dest }),
  pickSavePath: (name) => save({ defaultPath: `${name}.spiderlens`, filters: [{ name: "SpiderLens crawl", extensions: ["spiderlens"] }] }),
  recentList: () => invoke("recent_list"),
  recentRemove: (path) => invoke("recent_remove", { path }),
  recentLocate: (oldPath, newPath) => invoke("recent_locate", { old: oldPath, new: newPath }),
  unsavedList: () => invoke("unsaved_list"),
  unsavedDelete: (path) => invoke("unsaved_delete", { path }),
  crawlSalvage: (path) => invoke("crawl_salvage", { path }),
  storageStatus: () => invoke("storage_status"),
  pendingOpen: () => invoke("pending_open"),
  onOpenFile: (cb) => listen<string>("open-file", (e) => cb(e.payload)),
  onFileDrop: (cb) =>
    getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "drop") cb(e.payload.paths);
    }),
  onCrawlMoved: (cb) => listen<CrawlMoved>("crawl-moved", (e) => cb(e.payload)),
  onAppQuitting: (cb) => listen<{ pausing: boolean }>("app-quitting", (e) => cb(e.payload)),
  onCrawlsRecovered: (cb) => listen<RecentEntry[]>("crawls-recovered", (e) => cb(e.payload)),
  resultsTabs: () => invoke("results_tabs"),
  resultsOpen: (path) => invoke("results_open", { path }),
  viewOpen: (path, query, count) => invoke("view_open", { path, query, count }),
  viewRows: (path, view, from, count) => invoke("view_rows", { path, view, from, count }),
  viewLocate: (path, view, ids) => invoke("view_locate", { path, view, ids }),
  viewRefresh: (path, view, anchors) => invoke("view_refresh", { path, view, anchors }),
  viewCounts: (path, tab, scope) => invoke("view_counts", { path, tab, scope }),
  viewClose: (path, view) => invoke("view_close", { path, view }),
  urlDetail: (path, id) => invoke("url_detail", { path, id }),
  urlList: (path, id, kind, from, count) => invoke("url_list", { path, id, kind, from, count }),
  copyRows: (path, view, ranges, columns) => invoke("copy_rows", { path, view, ranges, columns }),
  exportStart: (path, query, columns, ranges, dest) => invoke("export_start", { path, query, columns, ranges, dest }),
  exportCancel: (job) => invoke("export_cancel", { job }),
  onViewReady: (cb) => listen<ViewReady>("view-ready", (e) => cb(e.payload)),
  onExportProgress: (cb) => listen<ExportProgress>("export-progress", (e) => cb(e.payload)),
  agentsPage: (path, id) => invoke("agents_page", { path, id }),
  assistantsGet: () => invoke("assistants_get"),
  assistantsSet: (settings) => invoke("assistants_set", { settings }),
  assistantsTest: () => invoke("assistants_test"),
  assistantsActivity: (limit) => invoke("assistants_activity", { limit }),
  assistantsActivityClear: () => invoke("assistants_activity_clear"),
  pickFolder: async () => asPath(await open({ directory: true })),
};
