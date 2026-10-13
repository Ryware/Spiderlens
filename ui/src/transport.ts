// The only module that knows how the UI talks to the engine. The desktop app
// uses Tauri commands; a future web version replaces this file with HTTP calls
// and leaves every component untouched (constitution principle III).

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { CrawlConfig } from "./generated/CrawlConfig";
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
}

export interface CrawlReport {
  path: string;
  summary: CrawlSummary;
  /** Not-crawled reasons with the engine's labels, in FR-038 order. */
  reasons: { reason: string; label: string; count: number }[];
}

export interface CrawlFinished {
  id: number;
  path: string;
  report: CrawlReport | null;
  error: string | null;
}

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
};
