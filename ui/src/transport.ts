// The only module that knows how the UI talks to the engine. The desktop app
// uses Tauri commands; a future web version replaces this file with HTTP calls
// and leaves every component untouched (constitution principle III).

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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

/** Positions [from, to) in a view: a selection. */
export type Range = [number, number];

export interface Transport {
  buildInfo(): Promise<BuildInfo>;
  defaults(builtin: boolean): Promise<CrawlConfig>;
  presets(): Promise<Presets>;
  validate(config: CrawlConfig, startUrl: string | null): Promise<Validation>;
  testPatterns(include: string[], exclude: string[], samples: string[]): Promise<PatternTest[]>;
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
  crawlResume(path: string): Promise<CrawlStarted>;
  crawlPause(id: number): Promise<void>;
  crawlStop(id: number): Promise<void>;
  crawlSummary(path: string): Promise<CrawlReport>;
  onCrawlProgress(cb: (p: CrawlProgress) => void): Promise<() => void>;
  onCrawlWarning(cb: (w: { id: number; message: string }) => void): Promise<() => void>;
  onCrawlFinished(cb: (f: CrawlFinished) => void): Promise<() => void>;
  pickCrawlFile(): Promise<string | null>;
  pickCsvPath(name: string): Promise<string | null>;
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
}

const asPath = (p: string | string[] | null) => (Array.isArray(p) ? (p[0] ?? null) : p);

export const tauriTransport: Transport = {
  buildInfo: () => invoke("build_info"),
  defaults: (builtin) => invoke("config_defaults", { builtin }),
  presets: () => invoke("config_presets"),
  validate: (config, startUrl) => invoke("config_validate", { config, startUrl }),
  testPatterns: (include, exclude, samples) => invoke("patterns_test", { include, exclude, samples }),
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
  crawlResume: (path) => invoke("crawl_resume", { path }),
  crawlPause: (id) => invoke("crawl_pause", { id }),
  crawlStop: (id) => invoke("crawl_stop", { id }),
  crawlSummary: (path) => invoke("crawl_summary", { path }),
  onCrawlProgress: (cb) => listen<CrawlProgress>("crawl-progress", (e) => cb(e.payload)),
  onCrawlWarning: (cb) => listen<{ id: number; message: string }>("crawl-warning", (e) => cb(e.payload)),
  onCrawlFinished: (cb) => listen<CrawlFinished>("crawl-finished", (e) => cb(e.payload)),
  pickCrawlFile: async () => asPath(await open({ filters: [{ name: "SpiderLens crawl", extensions: ["spiderlens"] }] })),
  pickCsvPath: (name) => save({ defaultPath: `${name}.csv`, filters: [{ name: "CSV", extensions: ["csv"] }] }),
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
};
