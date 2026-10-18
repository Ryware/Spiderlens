// The only module that knows how the UI talks to the engine. The desktop app
// uses Tauri commands; a future web version replaces this file with HTTP calls
// and leaves every component untouched (constitution principle III).

import type { PassProgress } from "./generated/PassProgress";
import type { AnalysisReport } from "./generated/AnalysisReport";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { ClustersPage } from "./generated/ClustersPage";
import type { Comparison } from "./generated/Comparison";
import type { ContentSettings } from "./generated/ContentSettings";
import type { AuthMethod } from "./generated/AuthMethod";
import type { CookieFilter } from "./generated/CookieFilter";
import type { ProxySettings } from "./generated/ProxySettings";
import type { CookiePage } from "./generated/CookiePage";
import type { CookieUrlPage } from "./generated/CookieUrlPage";
import type { CrawlConfig } from "./generated/CrawlConfig";
import type { CustomResult } from "./generated/CustomResult";
import type { CustomRules } from "./generated/CustomRules";
import type { MembersPage } from "./generated/MembersPage";
import type { NearDupStatus } from "./generated/NearDupStatus";
import type { PairsPage } from "./generated/PairsPage";
import type { Features } from "./generated/Features";
import type { FilterCounts } from "./generated/FilterCounts";
import type { HtmlDiff } from "./generated/HtmlDiff";
import type { HtmlVersion } from "./generated/HtmlVersion";
import type { ListKind } from "./generated/ListKind";
import type { ListPage } from "./generated/ListPage";
import type { PageHtml } from "./generated/PageHtml";
import type { Refresh } from "./generated/Refresh";
import type { RowPage } from "./generated/RowPage";
import type { TabCounts } from "./generated/TabCounts";
import type { TabDef } from "./generated/TabDef";
import type { UrlDetail } from "./generated/UrlDetail";
import type { UrlScope } from "./generated/UrlScope";
import type { ViewQuery } from "./generated/ViewQuery";
import type { Values } from "./generated/Values";
import type { CacheInfo } from "./generated/CacheInfo";
import type { Connection } from "./generated/Connection";
import type { OrphanPage } from "./generated/OrphanPage";
import type { PageRequest } from "./generated/PageRequest";
import type { Property } from "./generated/Property";
import type { ProviderSettings } from "./generated/ProviderSettings";
import type { ProviderStatus } from "./generated/ProviderStatus";
import type { QueryRow } from "./generated/QueryRow";
import type { RunProgress } from "./generated/RunProgress";
import type { Status as IntegrationStatus } from "./generated/Status";
import type { Scene } from "./generated/Scene";
import type { TermLinks } from "./generated/TermLinks";
import type { ExportDone } from "./generated/ExportDone";
import type { ExportFormat } from "./generated/ExportFormat";
import type { ExportOptions } from "./generated/ExportOptions";
import type { ViewState } from "./generated/ViewState";
import type { InvalidLine } from "./generated/InvalidLine";
import type { IssueSettings } from "./generated/IssueSettings";
import type { IssuesSummary } from "./generated/IssuesSummary";
import type { RuleCatalogue } from "./generated/RuleCatalogue";
import type { SettingsNotice } from "./generated/SettingsNotice";
import type { UrlIssue } from "./generated/UrlIssue";
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
import type { CompareSettings } from "./generated/CompareSettings";
import type { ComparisonReport } from "./generated/ComparisonReport";
import type { ElementDelta } from "./generated/ElementDelta";
import type { ElementSetting } from "./generated/ElementSetting";
import type { Group } from "./generated/Group";
import type { GroupPage } from "./generated/GroupPage";
import type { GroupSort } from "./generated/GroupSort";
import type { MapRule } from "./generated/MapRule";
import type { MappingTest } from "./generated/MappingTest";
import type { PairDetail } from "./generated/PairDetail";
import type { Setup } from "./generated/Setup";
import type { Summary } from "./generated/Summary";
import type { GenerateReport } from "./generated/GenerateReport";
import type { SitemapFiles } from "./generated/SitemapFiles";
import type { OrphanSummary } from "./generated/OrphanSummary";
import type { UrlSitemapEntry } from "./generated/UrlSitemapEntry";
import type { SitemapGenerateOptions } from "./generated/SitemapGenerateOptions";
import type { AnalysisProgress } from "./generated/AnalysisProgress";
import type { AnalysisRun } from "./generated/AnalysisRun";
import type { LinkAnalysisSettings } from "./generated/LinkAnalysisSettings";
import type { PositionRule } from "./generated/PositionRule";
import type { TestedLink } from "./generated/TestedLink";
import type { RetrySettings } from "./generated/RetrySettings";
import type { Dictionaries } from "./generated/Dictionaries";
import type { Installed } from "./generated/Installed";
import type { SpellingSettings } from "./generated/SpellingSettings";
import type { SpellSummary } from "./generated/SpellSummary";

/** A spelling check's progress, then its end (spec 027 part D). */
export interface SpellProgress {
  checked: number;
  total: number;
  state: "running" | "done" | "cancelled" | "failed";
  summary: SpellSummary | null;
  error: string | null;
}
import type { AeoRule } from "./generated/AeoRule";
import type { AiBot } from "./generated/AiBot";
import type { AiPageResult } from "./generated/AiPageResult";
import type { AiProgress } from "./generated/AiProgress";
import type { AiSummary } from "./generated/AiSummary";
import type { Frequency } from "./generated/Frequency";
import type { Preview } from "./generated/Preview";
import type { RepairReport } from "./generated/RepairReport";
import type { Run } from "./generated/Run";
import type { Schedule } from "./generated/Schedule";
import type { ScheduleDetail } from "./generated/ScheduleDetail";
import type { ScheduleRow } from "./generated/ScheduleRow";
import type { Segment } from "./generated/Segment";
import type { SegmentCounts } from "./generated/SegmentCounts";
import type { Report } from "./generated/Report";
import type { FileOptions } from "./generated/FileOptions";
import type { BulkOptions } from "./generated/BulkOptions";
import type { BulkProgress } from "./generated/BulkProgress";
import type { BulkResult } from "./generated/BulkResult";
import type { BulkFormat } from "./generated/BulkFormat";

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
  /** The segment the URL would be in by its address (spec 027 FR-B08). */
  segment?: string | null;
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
  /** URLs waiting to be retried (spec 004). */
  waiting: number;
  /** Sites paused because they kept failing (spec 004 FR-028). */
  paused_hosts: PausedHost[];
  /** Every remaining URL's site is paused: nothing can be requested (US4.4). */
  waiting_for_host: boolean;
  /** What keeps the crawl from going faster, if anything (spec 008 FR-028). */
  limit: SpeedLimit | null;
  /** Sites with work in progress, busiest first; at most 50 (FR-029). */
  sites: SitePace[];
  /** Sites with work in progress in all. */
  sites_total: number;
}

/** The main limit on the crawl's speed (spec 008 FR-028). */
export interface SpeedLimit {
  kind: "concurrency" | "rate" | "per_site" | "site_slowdown" | "site_overload" | "requested_wait" | "crawl_delay" | "site_paused" | "starting_up" | "connection" | "computer" | "saving";
  /** The engine's host key, when it is about one site. */
  host: string | null;
  /** For a requested wait or a pause: seconds until it ends. */
  seconds_left: number | null;
  /** In plain language. */
  message: string;
}

/** One row of the per-site speed breakdown (spec 008 FR-029). */
export interface SitePace {
  host: string;
  state: "speeding_up" | "steady" | "slowed_down" | "waiting" | "paused";
  /** Requests it may have open now, and the most it ever gets. */
  allowance: number;
  max: number;
  in_flight: number;
  /** Recent average response time. */
  response_ms: number | null;
  crawled: number;
  /** URLs waiting for it, counted in the part of the queue held in memory; more on disk when `waiting_more`. */
  waiting: number;
  waiting_more: boolean;
  reason: string;
  seconds_left: number | null;
}

/** A site the crawl has paused (spec 004 FR-028). `host` is the engine's host key. */
export interface PausedHost {
  host: string;
  /** Why, in words. */
  reason: string;
  /** Seconds until the test request; 0 while it is out. */
  seconds_left: number;
  /** The test request is out. */
  probing: boolean;
  /** Failed too often: its remaining URLs were recorded as "Host unavailable". */
  given_up: boolean;
}

/** Something worth telling the user while the crawl runs: a site paused, resumed or given up, or the network went down (spec 004). */
export interface CrawlNotice {
  id: number;
  kind: string;
  host: string | null;
  message: string;
}

/** Cookie names the login window captured, per domain; values never leave Rust (spec 025 FR-009). */
export interface LoginCaptured {
  sites: { domain: string; names: string[] }[];
  captured_at: number;
  login_url: string;
}

/** What importing a cookies.txt found (spec 025 US3 scenario 5). */
export interface CookiesImported {
  imported: number;
  skipped_expired: number;
  skipped_external: number;
  skipped_malformed: number;
  captured: LoginCaptured | null;
}

/** The proxy a crawl uses, as resolved when it starts (spec 025 FR-023). */
export interface ProxyInEffect {
  mode: "none" | "system" | "manual";
  kind: string | null;
  host: string | null;
  port: number | null;
  user: string | null;
  /** "manual", "env", "os", or "none" when the system has no proxy. */
  source: string;
  bypass: string[];
}

/** The proxy Test button's answer (spec 025 FR-022). */
export interface ProxyTestResult {
  ok: boolean;
  in_effect: ProxyInEffect | null;
  problem: "unreachable" | "rejected" | "pac_unreadable" | "pac_unsupported" | "invalid" | null;
  message: string | null;
}

/** What "Retry failed URLs" would do, before anything changes (spec 004 US5.1, FR-034). */
export interface RetryPreview {
  total: number;
  /** Counts per error class, with the engine's labels. */
  by_class: { class: string; label: string; count: number }[];
  available: boolean;
  /** Why it is unavailable, in words. */
  reason: string | null;
}

/** One crawl log entry, newest first (spec 004 FR-035). */
export interface CrawlLogEntry {
  at_ms: number;
  kind: string;
  host: string | null;
  url: string | null;
  message: string;
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
  /** Requested URLs by final error class, HTTP classes included (spec 004 FR-036). */
  by_class: Record<string, number>;
  transient_errors: number;
  permanent_errors: number;
  /** Automatic retries made. */
  retries: number;
  /** URLs that needed more than one attempt and ended without an error. */
  recovered: number;
  /** Sites paused at least once. */
  hosts_paused: number;
  /** "Retry failed URLs" runs. */
  manual_retries: number;
  /** Why the crawl paused itself, if it did: "network" or "local". */
  pause_reason: string | null;
  /** Sites that slowed down, asked to wait or paused, one entry per run (spec 008 FR-021). */
  speed_events?: { host: string; summary: string }[];
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

export type LocalErrorKind = "disk_low" | "disk_full" | "drive_gone" | "permission" | "network" | "other";

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

// Crawl data management (spec 019, contracts/desktop-commands.md).

export interface CrawlBackup {
  path: string;
  format_version: number;
  modified: string | null;
  size_bytes: number;
}

/** One crawl in Manage crawls: file metadata only; the stored-HTML size comes from htmlSize. */
export interface ManagedCrawl {
  path: string;
  name: string;
  start_url: string | null;
  status: string | null;
  urls: number | null;
  /** The crawl file and its helper files. */
  size_bytes: number;
  created: string | null;
  last_used: string | null;
  available: boolean;
  /** Written by a newer SpiderLens. */
  view_only: boolean;
  backups: CrawlBackup[];
  unsaved: boolean;
  running: boolean;
  in_recent: boolean;
}

export interface ManageList {
  crawls: ManagedCrawl[];
  /** Files in the unsaved-crawls folder that are not crawls; never removed automatically. */
  other_files: { path: string; size_bytes: number }[];
  totals: { all: number; unsaved: number; leftovers: number };
  free_bytes: number | null;
  unsaved_dir: string;
}

export interface HtmlSize {
  bytes: number;
  pages: number;
}

export interface DeleteReport {
  deleted: string[];
  skipped: { path: string; reason: string }[];
  /** Not deleted: no trash there. Ask again before deleting permanently. */
  trash_unavailable: string[];
  freed_bytes: number;
}

export interface PurgeCheck {
  html_bytes: number;
  html_pages: number;
  file_bytes: number;
  new_size: number;
  free_bytes: number | null;
  can_compact: boolean;
}

export interface PurgeReport {
  html_pages: number;
  html_bytes: number;
  freed_bytes: number;
  compacted: boolean;
}

export interface StoragePrefs {
  version: number;
  /** null: unsaved crawls are never removed automatically. */
  cleanup_days: number | null;
  reserve_bytes: number;
  first_cleanup_announced: string | null;
  last_cleanup: string | null;
}

export type CleanupReason = "manual_delete" | "manual_delete_permanent" | "auto_age" | "leftover" | "html_purge";

export interface CleanupRecord {
  at: string;
  path: string;
  bytes: number;
  reason: CleanupReason;
}

export interface CleanupReport {
  removed: string[];
  leftovers: number;
  freed_bytes: number;
  /** Named by the one-time notice: removed at a later start. */
  announced: string[];
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

/** Near duplicate analysis progress (spec 020). */
export interface NearDupProgress {
  path: string;
  done: number;
  total: number;
  step: string;
}

export interface NearDupFinished {
  path: string;
  summary: { compared: number; too_short: number; not_indexable: number; no_signature: number; clusters: number; ms: number } | null;
  cancelled: boolean;
  error: string | null;
}

export type NearDupExport = "pages" | "pairs" | "clusters";
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

/** A running comparison's progress (spec 012); ends with done, failed or cancelled. */
export interface CompareProgress {
  comparison: number;
  phase: "open" | "previous" | "current" | "match" | "counts" | "done" | "failed" | "cancelled";
  done: number;
  total: number;
  error: string | null;
}

// Custom extraction and search (spec 013, contracts/desktop-commands.md).

/** What one rule found on a tested page (FR-018). */
export interface RuleTestResult {
  name: string;
  kind: "extractor" | "search";
  count: number;
  flags: string[];
  values: string[];
  error: string | null;
}

export interface RuleTestOutcome {
  /** The page the rules ran on, after redirects. */
  url: string;
  /** Null for a stored page. */
  status: number | null;
  results: RuleTestResult[];
  ms: number;
}

/** A rule as stored with a crawl, and how it last ran (FR-013). */
export interface CrawlRule {
  name: string;
  kind: "extractor" | "search";
  last_run_at: string | null;
  run_pages: number | null;
  run_state: "crawl" | "complete" | "partial" | "stale" | null;
}

export interface CustomRunProgress {
  run: number;
  done: number;
  total: number;
  per_sec: number;
}

export interface CustomRunDone {
  run: number;
  path: string;
  state: "complete" | "partial" | "failed";
  pages: number;
  error: string | null;
}

// AI checks (spec 015, contracts/desktop-commands.md).

export type AeoProgress = AiProgress & { run_id: number; path: string };

export interface AeoFinished {
  run_id: number;
  path: string;
  /** Null when the run was cancelled or failed; earlier results stay. */
  summary: AiSummary | null;
  error: string | null;
}

export interface AiBotList {
  version: number;
  date: string;
  bots: AiBot[];
}

/** A saved schedule (spec 021): its detail, warnings, and why the OS refused the registration. */
export interface ScheduleSaved {
  detail: ScheduleDetail;
  warnings: Issue[];
  refused: string | null;
}

/** A report file written to disk (spec 010). */
export interface Written {
  path: string;
  bytes: number;
}

/** The error text when a target exists and `overwrite` was not set. */
export const EXISTS = "exists";

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
  pickSitemapFile(): Promise<string | null>;
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
  /** With JavaScript rendering on, rejects with "browser_unavailable:" plus a message when no browser is usable (spec 014). */
  crawlStart(config: CrawlConfig, startUrl: string | null): Promise<CrawlStarted>;
  /** Rejects with "needs_upgrade:", "needs_secrets:", "read_only:", "in_use:" or "browser_unavailable:" plus a message. */
  crawlResume(path: string, options?: ResumeOptions): Promise<CrawlStarted>;
  crawlPause(id: number): Promise<void>;
  crawlStop(id: number): Promise<void>;
  crawlSummary(path: string): Promise<CrawlReport>;
  onCrawlProgress(cb: (p: CrawlProgress) => void): Promise<() => void>;
  onCrawlWarning(cb: (w: { id: number; message: string }) => void): Promise<() => void>;
  /** The Crawl Analysis phase at crawl end or on open (spec 009 plan D12). */
  onPassProgress(cb: (p: PassProgress) => void): Promise<() => void>;
  /** Runs the analysis passes missing or stale on an opened file. */
  passesRun(path: string): Promise<AnalysisReport>;
  onCrawlFinished(cb: (f: CrawlFinished) => void): Promise<() => void>;
  onCrawlNotice(cb: (n: CrawlNotice) => void): Promise<() => void>;
  // Fetch errors and retries (spec 004, contracts/desktop-commands.md).
  crawlResumeHost(id: number, host: string): Promise<void>;
  crawlApplySettings(id: number, speed: SpeedSettings | null, retry: RetrySettings | null): Promise<void>;
  retryPreview(path: string): Promise<RetryPreview>;
  retryStart(path: string): Promise<CrawlStarted>;
  crawlLog(path: string, limit?: number): Promise<CrawlLogEntry[]>;
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
  // Crawl data management (spec 019).
  manageList(): Promise<ManageList>;
  htmlSize(path: string): Promise<HtmlSize>;
  revealPath(path: string): Promise<void>;
  /** Moves crawls and their helper files to the trash. Never deletes permanently. */
  crawlsDelete(paths: string[]): Promise<DeleteReport>;
  /** Only after a second confirmation, for paths the trash refused. */
  crawlsDeletePermanently(paths: string[]): Promise<DeleteReport>;
  purgeHtmlCheck(path: string): Promise<PurgeCheck>;
  purgeHtml(path: string, compact: boolean): Promise<PurgeReport>;
  storagePrefs(): Promise<StoragePrefs>;
  /** Rejects with a JSON array of Issue when a value is out of range. */
  setStoragePrefs(prefs: StoragePrefs): Promise<StoragePrefs>;
  cleanupHistory(): Promise<CleanupRecord[]>;
  onCleanupDone(cb: (r: CleanupReport) => void): Promise<() => void>;
  onCrawlsChanged(cb: () => void): Promise<() => void>;
  // Results (spec 007, contracts/desktop-commands.md).
  /** With a crawl file, its custom extraction and search tabs too (spec 013). */
  resultsTabs(path?: string | null): Promise<TabDef[]>;
  resultsOpen(path: string): Promise<{ features: Features }>;
  viewOpen(path: string, query: ViewQuery, count: number): Promise<RowPage>;
  viewRows(path: string, view: number, from: number, count: number): Promise<RowPage>;
  viewLocate(path: string, view: number, ids: number[]): Promise<(number | null)[]>;
  viewRefresh(path: string, view: number, anchors: number[]): Promise<Refresh | null>;
  /** `segment`: only URLs in that segment; 0 means in no segment (spec 027). `values`: raw or rendered values of a rendered crawl (spec 014); omitted means rendered when it has rendering. */
  viewCounts(path: string, tab: string, scope: UrlScope, segment?: number, values?: Values): Promise<FilterCounts>;
  /** URLs each result tab lists, for the sidebar. */
  tabCounts(path: string): Promise<TabCounts>;
  /** Groups the open crawl's URLs by these segments; the file is not changed. */
  segmentsSet(path: string, segments: Segment[]): Promise<SegmentCounts>;
  segmentsCounts(path: string): Promise<SegmentCounts>;
  viewClose(path: string, view: number): Promise<void>;
  urlDetail(path: string, id: number): Promise<UrlDetail>;
  /** `position` keeps only links at that position (spec 023 FR-024). `values`: raw or rendered lists of a rendered crawl (spec 014); omitted means rendered when the crawl has rendering. */
  urlList(path: string, id: number, kind: ListKind, from: number, count: number, position?: string | null, values?: Values): Promise<ListPage>;
  // Site login and proxy (spec 025, contracts/desktop-commands.md).
  /** Opens the login page in a private window; its addresses arrive through onLoginNavigated. */
  loginOpen(loginUrl: string, internalHosts: string[], userAgent: string | null): Promise<void>;
  /** Keeps the window's cookies for the crawl's own hosts and closes it. */
  loginDone(): Promise<LoginCaptured>;
  loginCancel(): Promise<void>;
  /** Forgets the captured session. */
  loginClear(): Promise<void>;
  loginStatus(): Promise<LoginCaptured | null>;
  onLoginNavigated(cb: (url: string) => void): Promise<() => void>;
  onLoginClosed(cb: () => void): Promise<() => void>;
  pickCookiesFile(): Promise<string | null>;
  cookiesImport(path: string, internalHosts: string[]): Promise<CookiesImported>;
  /** Adds a sign-in to a running crawl and re-queues the origin's 401 URLs; resolves to how many. */
  crawlAddSignIn(id: number, origin: string, user: string, password: string, method: AuthMethod | null): Promise<number>;
  proxyTest(proxy: ProxySettings, probeUrl: string): Promise<ProxyTestResult>;
  // Cookie audit (spec 025, contracts/desktop-commands.md).
  /** Distinct cookies the crawl saw, without values; sort is a column name, default URLs descending. */
  cookiesList(path: string, filter: CookieFilter, sort: string | null, descending: boolean, offset: number, limit: number): Promise<CookiePage>;
  /** The URLs that set one cookie, in crawl order. */
  cookieUrls(path: string, name: string, domain: string, cookiePath: string, offset: number, limit: number): Promise<CookieUrlPage>;
  /** The page's stored HTML, or why there is none. `version`: the HTML as received (default) or as rendered (spec 014 FR-032). */
  urlHtml(path: string, id: number, version?: HtmlVersion): Promise<PageHtml>;
  /** The normalised raw and rendered HTML compared: `maxHunks` differences from number `fromHunk` (spec 014 FR-028). Rejects when either version is not stored. */
  urlHtmlDiff(path: string, id: number, fromHunk: number, maxHunks: number): Promise<HtmlDiff>;
  /** The tabs that list the URL unfiltered, in display order; empty when none does. */
  urlTabs(path: string, id: number): Promise<string[]>;
  /** Puts one value on the clipboard. */
  copyText(text: string): Promise<void>;
  /** Puts the rows on the clipboard as TSV with a header and returns that text. */
  copyRows(path: string, view: number, ranges: Range[], columns: string[]): Promise<string>;
  /** `allValues`: custom extraction columns hold every value joined by it (spec 013 FR-025). */
  exportStart(path: string, query: ViewQuery, columns: string[], ranges: Range[] | null, dest: string, allValues?: string | null, segments?: Segment[]): Promise<number>;
  exportCancel(job: number): Promise<void>;
  // Sitemap generation (spec 011, contracts/desktop-commands.md).
  sitemapFiles(path: string): Promise<SitemapFiles>;
  /** Spec 011 FR-012: the orphan counts above the Sitemaps tab. */
  orphanSummary(path: string): Promise<OrphanSummary>;
  /** Spec 011 FR-026: the sitemaps that list one URL. */
  sitemapEntriesForUrl(path: string, id: number): Promise<UrlSitemapEntry[]>;
  sitemapPreview(path: string, options: SitemapGenerateOptions, folder: string | null): Promise<GenerateReport>;
  sitemapGenerate(path: string, options: SitemapGenerateOptions, folder: string, replace: boolean): Promise<GenerateReport>;
  /** A visualisation scene (spec 022): the crawl or directory tree as `view` asks. */
  vizScene(path: string, view: ViewState): Promise<Scene>;
  /** The links behind one anchor text term (spec 022 FR-018); the first `limit` (default 500). */
  vizTermLinks(path: string, view: ViewState, term: string, limit?: number): Promise<TermLinks>;
  /** Writes the view to `file` as SVG, PNG or CSV (spec 022 FR-024). */
  vizExport(path: string, view: ViewState, options: ExportOptions, file: string): Promise<ExportDone>;
  pickVizPath(name: string, format: ExportFormat): Promise<string | null>;
  /** Runs Crawl Analysis (link positions, Link Score) on the open file now. */
  analysisRun(path: string): Promise<AnalysisRun>;
  /** Starts Crawl Analysis in the background: progress and done arrive as events. */
  analysisStart(path: string): Promise<void>;
  /** Cancels the running Crawl Analysis; earlier results stay. */
  analysisCancel(): Promise<void>;
  onAnalysisProgress(cb: (p: AnalysisProgressEvent) => void): Promise<() => void>;
  onAnalysisDone(cb: (d: AnalysisDone) => void): Promise<() => void>;
  /** Uses these link analysis settings for the open file from now on. */
  analysisSettings(path: string, settings: LinkAnalysisSettings): Promise<void>;
  /** The rules tester: null when the URL is not in the crawl or its HTML was not stored. */
  positionsTest(path: string, url: string, rules: PositionRule[]): Promise<TestedLink[] | null>;
  onViewReady(cb: (r: ViewReady) => void): Promise<() => void>;
  onExportProgress(cb: (p: ExportProgress) => void): Promise<() => void>;
  // Near duplicate content (spec 020, contracts/desktop-commands.md).
  nearDupStatus(path: string): Promise<NearDupStatus>;
  nearDupClusters(path: string, offset: number, limit: number): Promise<ClustersPage>;
  nearDupMembers(path: string, cluster: number, offset: number, limit: number): Promise<MembersPage>;
  nearDupPairs(path: string, urlId: number): Promise<PairsPage>;
  comparePages(path: string, a: number, b: number, full: boolean): Promise<Comparison>;
  /** Starts the analysis; `settings` re-runs with changed content settings. */
  runNearDup(path: string, settings: ContentSettings | null): Promise<void>;
  cancelNearDup(path: string): Promise<void>;
  exportNearDup(path: string, kind: NearDupExport, cluster: number | null, dest: string): Promise<number>;
  onNearDupProgress(cb: (p: NearDupProgress) => void): Promise<() => void>;
  onNearDupFinished(cb: (f: NearDupFinished) => void): Promise<() => void>;
  // AI agents (spec 016, contracts/desktop-commands.md).
  agentsPage(path: string, id: number): Promise<PageAgentFindings>;
  assistantsGet(): Promise<AssistantsView>;
  /** Rejects with a sentence when a folder does not exist. */
  assistantsSet(settings: AssistantSettings): Promise<AssistantSettings>;
  /** Rejects with a sentence when the server does not start or answer. */
  assistantsTest(): Promise<McpSelfTest>;
  assistantsActivity(limit: number): Promise<ActivityEntry[]>;
  assistantsActivityClear(): Promise<void>;

  // Spelling (spec 027 part D)
  spellDictionaries(): Promise<Dictionaries>;
  /** Downloads a listed dictionary, checked against its published checksum. */
  spellDownload(lang: string): Promise<Installed>;
  /** Asks for a .aff and then a .dic file; null when either is cancelled. */
  pickDictionaryFiles(): Promise<{ aff: string; dic: string } | null>;
  spellAdd(lang: string, aff: string, dic: string): Promise<Installed>;
  spellRemove(lang: string): Promise<void>;
  /** Starts checking a crawl file; progress comes through onSpellProgress. */
  spellStart(path: string, settings: SpellingSettings): Promise<void>;
  spellCancel(): Promise<void>;
  spellSummary(path: string): Promise<SpellSummary | null>;
  onSpellProgress(cb: (p: SpellProgress) => void): Promise<() => void>;
  // Crawl comparison (spec 012, contracts/desktop-commands.md).
  compareSuggest(current: string): Promise<string | null>;
  compareSetup(previous: string, current: string): Promise<Setup>;
  compareTestMapping(previous: string, current: string, rules: MapRule[], samples: string[], fromPrevious: number): Promise<MappingTest[]>;
  compareStart(previous: string, current: string, settings: CompareSettings): Promise<number>;
  compareCancel(comparison: number): Promise<void>;
  compareSummary(comparison: number): Promise<Summary>;
  compareSetElements(comparison: number, elements: ElementSetting[]): Promise<ElementDelta[]>;
  compareOpenGroup(comparison: number, group: Group, search: string, sort: GroupSort | null, count: number): Promise<GroupPage>;
  compareRows(comparison: number, view: number, from: number, count: number): Promise<GroupPage>;
  compareCloseGroup(comparison: number, view: number): Promise<void>;
  compareDetail(comparison: number, prevId: number | null, curId: number | null): Promise<PairDetail>;
  compareReport(comparison: number): Promise<ComparisonReport>;
  /** CSV of an open group list; progress arrives as export-progress events. */
  compareExport(comparison: number, view: number, dest: string): Promise<number>;
  compareClose(comparison: number): Promise<void>;
  onCompareProgress(cb: (p: CompareProgress) => void): Promise<() => void>;
  // Issues (spec 009, contracts/desktop-commands.md).
  ruleCatalogue(settings: IssueSettings): Promise<RuleCatalogue>;
  /** Makes these settings the ones in force; errors reject the whole change. */
  issuesApply(path: string | null, settings: IssueSettings): Promise<SettingsNotice[]>;
  issuesSummary(path: string, running: boolean): Promise<IssuesSummary>;
  /** Counts so far while a long issuesSummary evaluation runs (FR-008). */
  onIssuesPartial(cb: (p: { path: string; summary: IssuesSummary }) => void): Promise<() => void>;
  urlIssues(path: string, id: number): Promise<UrlIssue[]>;
  // Custom extraction and search (spec 013).
  /** Every rule's result on one page, for the detail pane. */
  urlCustom(path: string, id: number): Promise<CustomResult[]>;
  customTestUrl(rules: CustomRules, url: string, config: CrawlConfig): Promise<RuleTestOutcome>;
  customTestPage(rules: CustomRules, crawlPath: string, urlId: number): Promise<RuleTestOutcome>;
  customRulesOfCrawl(crawlPath: string): Promise<CrawlRule[]>;
  /** Runs `rules`, or the rules stored with the crawl when null, on its stored HTML. */
  customRun(crawlPath: string, rules: CustomRules | null, only?: string[]): Promise<number>;
  customCancel(run: number): Promise<void>;
  onCustomRunProgress(cb: (p: CustomRunProgress) => void): Promise<() => void>;
  onCustomRunDone(cb: (d: CustomRunDone) => void): Promise<() => void>;
  pickRulesFile(): Promise<string | null>;
  pickRulesSavePath(name: string): Promise<string | null>;
  customImport(path: string): Promise<CustomRules>;
  customExport(path: string, rules: CustomRules): Promise<void>;
  // External data (spec 024, contracts/desktop-commands.md).
  integrationsStatus(): Promise<IntegrationStatus>;
  integrationsSettingsSave(provider: string, settings: ProviderSettings): Promise<ProviderStatus>;
  /** Saves the user's own Google client; an empty id goes back to the built-in one. */
  integrationsGoogleClient(clientId: string, clientSecret: string | null): Promise<void>;
  /** Opens Google's consent page in the system browser and resolves when the user answers. */
  integrationsConnectGoogle(features: string[]): Promise<Connection>;
  integrationsConnectCancel(): Promise<void>;
  integrationsSaveKey(provider: string, label: string, key: string): Promise<Connection>;
  /** True when Google confirmed the revoke. */
  integrationsForget(kind: string, label: string): Promise<boolean>;
  integrationsProperties(account: string | null, path: string | null): Promise<Property[]>;
  integrationsDefaultRequest(property: string): Promise<PageRequest>;
  integrationsRunSearchConsole(path: string, account: string | null, request: PageRequest): Promise<RunProgress>;
  integrationsRunResume(path: string, runId: number | null): Promise<RunProgress>;
  integrationsRunPause(): Promise<void>;
  integrationsRunCancel(): Promise<void>;
  integrationsRuns(path: string): Promise<RunProgress[]>;
  integrationsQueries(path: string, urlId: number): Promise<QueryRow[]>;
  integrationsOrphans(path: string, source: string | null, offset: number, limit: number): Promise<OrphanPage>;
  integrationsRemove(path: string, provider: string | null): Promise<void>;
  integrationsCacheInfo(): Promise<CacheInfo>;
  integrationsCacheClear(provider: string | null): Promise<number>;
  integrationsOpenTerms(provider: string): Promise<void>;
  onIntegrationsProgress(cb: (p: RunProgress) => void): Promise<() => void>;
  // AI checks (spec 015).
  /** Runs the checks on a crawl file; results replace the previous run's when it finishes. */
  aeoRun(path: string): Promise<{ run_id: number }>;
  aeoCancel(runId: number): Promise<void>;
  /** Null when the checks have not run on this file. */
  aeoSummary(path: string): Promise<AiSummary | null>;
  aeoPage(path: string, urlId: number): Promise<AiPageResult | null>;
  aeoLlmsTxtDraft(path: string, maxLinks: number): Promise<{ text: string }>;
  aeoRules(): Promise<AeoRule[]>;
  aiBotsList(): Promise<AiBotList>;
  onAeoProgress(cb: (p: AeoProgress) => void): Promise<() => void>;
  onAeoFinished(cb: (f: AeoFinished) => void): Promise<() => void>;
  // Scheduled crawls (spec 021, contracts/desktop-commands.md).
  schedulesList(): Promise<ScheduleRow[]>;
  scheduleGet(id: string): Promise<ScheduleDetail>;
  schedulePreview(frequency: Frequency): Promise<Preview>;
  scheduleValidate(draft: Schedule): Promise<Issue[]>;
  /** Rejects with the errors' messages when the draft is invalid. */
  scheduleSave(draft: Schedule): Promise<ScheduleSaved>;
  scheduleSetEnabled(id: string, enabled: boolean): Promise<ScheduleSaved>;
  scheduleDuplicate(id: string, name: string): Promise<Schedule>;
  scheduleDelete(id: string): Promise<void>;
  scheduleRunNow(id: string): Promise<void>;
  scheduleStop(id: string): Promise<void>;
  scheduleHistory(id: string, limit: number): Promise<Run[]>;
  scheduleUpdateFromProfile(id: string): Promise<ScheduleSaved>;
  schedulesRepair(): Promise<RepairReport>;
  schedulesRemoveAll(): Promise<number>;
  scheduleDefaultOutput(): Promise<string | null>;
  pickFolder(): Promise<string | null>;

  // Report and exports (spec 010, contracts/desktop-commands.md).
  report(path: string, running: boolean, profile: string | null): Promise<Report>;
  exportReportHtml(path: string, dest: string, file: FileOptions, overwrite: boolean): Promise<Written>;
  printReport(path: string, file: FileOptions): Promise<void>;
  bulkExport(path: string, options: BulkOptions): Promise<BulkResult>;
  bulkExportCancel(): Promise<void>;
  onBulkProgress(cb: (p: BulkProgress) => void): Promise<() => void>;
  pickReportPath(name: string): Promise<string | null>;
  pickBulkPath(name: string, format: BulkFormat): Promise<string | null>;
}

/** `analysis-progress` (spec 023 FR-013). */
export type AnalysisProgressEvent = AnalysisProgress & { path: string };
/** `analysis-done`: `run` is null when cancelled or failed. */
export type AnalysisDone = { path: string; run: AnalysisRun | null; cancelled: boolean; error: string | null };

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
  pickSitemapFile: async () =>
    asPath(await open({ filters: [{ name: "XML sitemaps", extensions: ["xml", "gz"] }] })),
  pickProfileFile: async () =>
    asPath(await open({ filters: [{ name: "SpiderLens profile", extensions: ["json"] }] })),
  pickExportPath: (name) =>
    save({ defaultPath: `${name}.spiderlens-profile.json`, filters: [{ name: "SpiderLens profile", extensions: ["json"] }] }),
  pickFolder: async () => asPath(await open({ directory: true })),
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
  onPassProgress: (cb) => listen<PassProgress>("passes-progress", (e) => cb(e.payload)),
  passesRun: (path) => invoke("passes_run", { path }),
  onCrawlFinished: (cb) => listen<CrawlFinished>("crawl-finished", (e) => cb(e.payload)),
  onCrawlNotice: (cb) => listen<CrawlNotice>("crawl-notice", (e) => cb(e.payload)),
  crawlResumeHost: (id, host) => invoke("crawl_resume_host", { id, host }),
  crawlApplySettings: (id, speed, retry) => invoke("crawl_apply_settings", { id, speed, retry }),
  retryPreview: (path) => invoke("retry_preview", { path }),
  retryStart: (path) => invoke("retry_start", { path }),
  crawlLog: (path, limit) => invoke("crawl_log", { path, limit }),
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
  manageList: () => invoke("crawls_manage_list"),
  htmlSize: (path) => invoke("crawls_html_size", { path }),
  revealPath: (path) => invoke("reveal_path", { path }),
  crawlsDelete: (paths) => invoke("crawls_delete", { paths }),
  crawlsDeletePermanently: (paths) => invoke("crawls_delete_permanently", { paths }),
  purgeHtmlCheck: (path) => invoke("crawls_purge_html_check", { path }),
  purgeHtml: (path, compact) => invoke("crawls_purge_html", { path, compact }),
  storagePrefs: () => invoke("storage_prefs_get"),
  setStoragePrefs: (prefs) => invoke("storage_prefs_set", { prefs }),
  cleanupHistory: () => invoke("cleanup_history"),
  onCleanupDone: (cb) => listen<CleanupReport>("cleanup-done", (e) => cb(e.payload)),
  onCrawlsChanged: (cb) => listen("crawls-changed", () => cb()),
  resultsTabs: (path) => invoke("results_tabs", { path: path ?? null }),
  resultsOpen: (path) => invoke("results_open", { path }),
  viewOpen: (path, query, count) => invoke("view_open", { path, query, count }),
  viewRows: (path, view, from, count) => invoke("view_rows", { path, view, from, count }),
  viewLocate: (path, view, ids) => invoke("view_locate", { path, view, ids }),
  viewRefresh: (path, view, anchors) => invoke("view_refresh", { path, view, anchors }),
  viewCounts: (path, tab, scope, segment, values) => invoke("view_counts", { path, tab, scope, segment: segment ?? null, values: values ?? null }),
  tabCounts: (path) => invoke("tab_counts", { path }),
  segmentsSet: (path, segments) => invoke("segments_set", { path, segments }),
  segmentsCounts: (path) => invoke("segments_counts", { path }),
  viewClose: (path, view) => invoke("view_close", { path, view }),
  urlDetail: (path, id) => invoke("url_detail", { path, id }),
  urlList: (path, id, kind, from, count, position, values) => invoke("url_list", { path, id, kind, from, count, position: position ?? null, values: values ?? null }),
  loginOpen: (loginUrl, internalHosts, userAgent) => invoke("login_open", { loginUrl, internalHosts, userAgent }),
  loginDone: () => invoke("login_done"),
  loginCancel: () => invoke("login_cancel"),
  loginClear: () => invoke("login_clear"),
  loginStatus: () => invoke("login_status"),
  onLoginNavigated: (cb) => listen<{ url: string }>("login://navigated", (e) => cb(e.payload.url)),
  onLoginClosed: (cb) => listen("login://closed", () => cb()),
  pickCookiesFile: async () => asPath(await open({ filters: [{ name: "cookies.txt", extensions: ["txt"] }] })),
  cookiesImport: (path, internalHosts) => invoke("cookies_import", { path, internalHosts }),
  crawlAddSignIn: (id, origin, user, password, method) => invoke("crawl_add_sign_in", { id, origin, user, password, method }),
  proxyTest: (proxy, probeUrl) => invoke("proxy_test", { proxy, probeUrl }),
  cookiesList: (path, filter, sort, descending, offset, limit) => invoke("cookies_list", { path, filter, sort, descending, offset, limit }),
  cookieUrls: (path, name, domain, cookiePath, offset, limit) => invoke("cookie_urls", { path, name, domain, cookiePath, offset, limit }),
  urlHtml: (path, id, version) => invoke("url_html", { path, id, version: version ?? null }),
  urlHtmlDiff: (path, id, fromHunk, maxHunks) => invoke("url_html_diff", { path, id, fromHunk, maxHunks }),
  urlTabs: (path, id) => invoke("url_tabs", { path, id }),
  copyText: (text) => invoke("copy_text", { text }),
  copyRows: (path, view, ranges, columns) => invoke("copy_rows", { path, view, ranges, columns }),
  exportStart: (path, query, columns, ranges, dest, allValues, segments) => invoke("export_start", { path, query, columns, ranges, dest, allValues: allValues ?? null, segments: segments ?? null }),
  exportCancel: (job) => invoke("export_cancel", { job }),
  sitemapFiles: (path) => invoke("sitemap_files", { path }),
  orphanSummary: (path) => invoke("orphan_summary", { path }),
  sitemapEntriesForUrl: (path, id) => invoke("sitemap_entries_for_url", { path, id }),
  sitemapPreview: (path, options, folder) => invoke("sitemap_preview", { path, options, folder }),
  sitemapGenerate: (path, options, folder, replace) => invoke("sitemap_generate", { path, options, folder, replace }),
  vizScene: (path, view) => invoke("viz_scene", { path, view }),
  vizTermLinks: (path, view, term, limit) => invoke("viz_term_links", { path, view, term, limit: limit ?? null }),
  vizExport: (path, view, options, file) => invoke("viz_export", { path, view, options, file }),
  pickVizPath: (name, format) =>
    save({
      defaultPath: `${name}.${format}`,
      filters: [{ name: format === "csv" ? "CSV" : `${format.toUpperCase()} image`, extensions: [format] }],
    }),
  analysisRun: (path) => invoke("analysis_run", { path }),
  analysisStart: (path) => invoke("analysis_start", { path }),
  analysisCancel: () => invoke("analysis_cancel"),
  onAnalysisProgress: (cb) => listen<AnalysisProgressEvent>("analysis-progress", (e) => cb(e.payload)),
  onAnalysisDone: (cb) => listen<AnalysisDone>("analysis-done", (e) => cb(e.payload)),
  analysisSettings: (path, settings) => invoke("analysis_settings", { path, settings }),
  positionsTest: (path, url, rules) => invoke("positions_test", { path, url, rules }),
  onViewReady: (cb) => listen<ViewReady>("view-ready", (e) => cb(e.payload)),
  onExportProgress: (cb) => listen<ExportProgress>("export-progress", (e) => cb(e.payload)),
  nearDupStatus: (path) => invoke("near_dup_status", { path, settings: null }),
  nearDupClusters: (path, offset, limit) => invoke("near_dup_clusters", { path, offset, limit }),
  nearDupMembers: (path, cluster, offset, limit) => invoke("near_dup_members", { path, cluster, offset, limit }),
  nearDupPairs: (path, urlId) => invoke("near_dup_pairs", { path, urlId }),
  comparePages: (path, a, b, full) => invoke("compare_pages", { path, a, b, full }),
  runNearDup: (path, settings) => invoke("run_near_dup", { path, settings }),
  cancelNearDup: (path) => invoke("cancel_near_dup", { path }),
  exportNearDup: (path, kind, cluster, dest) => invoke("export_near_dup", { path, kind, cluster, dest }),
  onNearDupProgress: (cb) => listen<NearDupProgress>("near-dup-progress", (e) => cb(e.payload)),
  onNearDupFinished: (cb) => listen<NearDupFinished>("near-dup-finished", (e) => cb(e.payload)),
  agentsPage: (path, id) => invoke("agents_page", { path, id }),
  assistantsGet: () => invoke("assistants_get"),
  assistantsSet: (settings) => invoke("assistants_set", { settings }),
  assistantsTest: () => invoke("assistants_test"),
  assistantsActivity: (limit) => invoke("assistants_activity", { limit }),
  assistantsActivityClear: () => invoke("assistants_activity_clear"),
  spellDictionaries: () => invoke("spell_dictionaries"),
  spellDownload: (lang) => invoke("spell_download", { lang }),
  pickDictionaryFiles: async () => {
    const aff = asPath(await open({ title: "Affix file (.aff)", filters: [{ name: "Hunspell affix file", extensions: ["aff"] }] }));
    if (!aff) return null;
    const dic = asPath(await open({ title: "Word list (.dic)", filters: [{ name: "Hunspell word list", extensions: ["dic"] }] }));
    return dic ? { aff, dic } : null;
  },
  spellAdd: (lang, aff, dic) => invoke("spell_add", { lang, aff, dic }),
  spellRemove: (lang) => invoke("spell_remove", { lang }),
  spellStart: (path, settings) => invoke("spell_start", { path, settings }),
  spellCancel: () => invoke("spell_cancel"),
  spellSummary: (path) => invoke("spell_summary", { path }),
  onSpellProgress: (cb) => listen<SpellProgress>("spell-progress", (e) => cb(e.payload)),
  compareSuggest: (current) => invoke("compare_suggest", { current }),
  compareSetup: (previous, current) => invoke("compare_setup", { previous, current }),
  compareTestMapping: (previous, current, rules, samples, fromPrevious) =>
    invoke("compare_test_mapping", { previous, current, rules, samples, fromPrevious }),
  compareStart: (previous, current, settings) => invoke("compare_start", { previous, current, settings }),
  compareCancel: (comparison) => invoke("compare_cancel", { comparison }),
  compareSummary: (comparison) => invoke("compare_summary", { comparison }),
  compareSetElements: (comparison, elements) => invoke("compare_set_elements", { comparison, elements }),
  compareOpenGroup: (comparison, group, search, sort, count) => invoke("compare_open_group", { comparison, group, search, sort, count }),
  compareRows: (comparison, view, from, count) => invoke("compare_rows", { comparison, view, from, count }),
  compareCloseGroup: (comparison, view) => invoke("compare_close_group", { comparison, view }),
  compareDetail: (comparison, prevId, curId) => invoke("compare_detail", { comparison, prevId, curId }),
  compareReport: (comparison) => invoke("compare_report", { comparison }),
  compareExport: (comparison, view, dest) => invoke("compare_export", { comparison, view, dest }),
  compareClose: (comparison) => invoke("compare_close", { comparison }),
  onCompareProgress: (cb) => listen<CompareProgress>("compare-progress", (e) => cb(e.payload)),
  ruleCatalogue: (settings) => invoke("rule_catalogue", { settings }),
  issuesApply: (path, settings) => invoke("issues_apply", { path, settings }),
  issuesSummary: (path, running) => invoke("issues_summary", { path, running }),
  onIssuesPartial: (cb) => listen<{ path: string; summary: IssuesSummary }>("issues-partial", (e) => cb(e.payload)),
  urlIssues: (path, id) => invoke("url_issues", { path, id }),
  urlCustom: (path, id) => invoke("url_custom", { path, id }),
  customTestUrl: (rules, url, config) => invoke("custom_test_url", { rules, url, config }),
  customTestPage: (rules, crawlPath, urlId) => invoke("custom_test_page", { rules, crawlPath, urlId }),
  customRulesOfCrawl: (crawlPath) => invoke("custom_rules_of_crawl", { crawlPath }),
  customRun: (crawlPath, rules, only) => invoke("custom_run", { crawlPath, rules, only: only ?? null }),
  customCancel: (run) => invoke("custom_cancel", { run }),
  onCustomRunProgress: (cb) => listen<CustomRunProgress>("custom-run-progress", (e) => cb(e.payload)),
  onCustomRunDone: (cb) => listen<CustomRunDone>("custom-run-done", (e) => cb(e.payload)),
  pickRulesFile: async () => asPath(await open({ filters: [{ name: "SpiderLens rules", extensions: ["json"] }] })),
  pickRulesSavePath: (name) => save({ defaultPath: `${name}.spiderlens-rules.json`, filters: [{ name: "SpiderLens rules", extensions: ["json"] }] }),
  customImport: (path) => invoke("custom_import", { path }),
  customExport: (path, rules) => invoke("custom_export", { path, rules }),
  integrationsStatus: () => invoke("integrations_status"),
  integrationsSettingsSave: (provider, settings) => invoke("integrations_settings_save", { provider, settings }),
  integrationsGoogleClient: (clientId, clientSecret) => invoke("integrations_google_client", { clientId, clientSecret }),
  integrationsConnectGoogle: (features) => invoke("integrations_connect_google", { features }),
  integrationsConnectCancel: () => invoke("integrations_connect_cancel"),
  integrationsSaveKey: (provider, label, key) => invoke("integrations_save_key", { provider, label, key }),
  integrationsForget: (kind, label) => invoke("integrations_forget", { kind, label }),
  integrationsProperties: (account, path) => invoke("integrations_properties", { account, path }),
  integrationsDefaultRequest: (property) => invoke("integrations_default_request", { property }),
  integrationsRunSearchConsole: (path, account, request) => invoke("integrations_run_search_console", { path, account, request }),
  integrationsRunResume: (path, runId) => invoke("integrations_run_resume", { path, runId }),
  integrationsRunPause: () => invoke("integrations_run_pause"),
  integrationsRunCancel: () => invoke("integrations_run_cancel"),
  integrationsRuns: (path) => invoke("integrations_runs", { path }),
  integrationsQueries: (path, urlId) => invoke("integrations_queries", { path, urlId }),
  integrationsOrphans: (path, source, offset, limit) => invoke("integrations_orphans", { path, source, offset, limit }),
  integrationsRemove: (path, provider) => invoke("integrations_remove", { path, provider }),
  integrationsCacheInfo: () => invoke("integrations_cache_info"),
  integrationsCacheClear: (provider) => invoke("integrations_cache_clear", { provider }),
  integrationsOpenTerms: (provider) => invoke("integrations_open_terms", { provider }),
  onIntegrationsProgress: (cb) => listen<RunProgress>("integrations://progress", (e) => cb(e.payload)),
  aeoRun: (path) => invoke("aeo_run", { path }),
  aeoCancel: (runId) => invoke("aeo_cancel", { runId }),
  aeoSummary: (path) => invoke("aeo_summary", { path }),
  aeoPage: (path, urlId) => invoke("aeo_page", { path, urlId }),
  aeoLlmsTxtDraft: (path, maxLinks) => invoke("aeo_llms_txt_draft", { path, maxLinks }),
  aeoRules: () => invoke("aeo_rules"),
  aiBotsList: () => invoke("ai_bots_list"),
  onAeoProgress: (cb) => listen<AeoProgress>("aeo-progress", (e) => cb(e.payload)),
  onAeoFinished: (cb) => listen<AeoFinished>("aeo-finished", (e) => cb(e.payload)),
  schedulesList: () => invoke("schedules_list"),
  scheduleGet: (id) => invoke("schedule_get", { id }),
  schedulePreview: (frequency) => invoke("schedule_preview", { frequency }),
  scheduleValidate: (draft) => invoke("schedule_validate", { draft }),
  scheduleSave: (draft) => invoke("schedule_save", { draft }),
  scheduleSetEnabled: (id, enabled) => invoke("schedule_set_enabled", { id, enabled }),
  scheduleDuplicate: (id, name) => invoke("schedule_duplicate", { id, name }),
  scheduleDelete: (id) => invoke("schedule_delete", { id }),
  scheduleRunNow: (id) => invoke("schedule_run_now", { id }),
  scheduleStop: (id) => invoke("schedule_stop", { id }),
  scheduleHistory: (id, limit) => invoke("schedule_history", { id, limit }),
  scheduleUpdateFromProfile: (id) => invoke("schedule_update_from_profile", { id }),
  schedulesRepair: () => invoke("schedules_repair"),
  schedulesRemoveAll: () => invoke("schedules_remove_all"),
  scheduleDefaultOutput: () => invoke("schedule_default_output"),

  report: (path, running, profile) => invoke("report", { path, running, profile }),
  exportReportHtml: (path, dest, file, overwrite) => invoke("export_report_html", { path, dest, file, overwrite }),
  printReport: (path, file) => invoke("print_report", { path, file }),
  bulkExport: (path, options) => invoke("bulk_export", { path, options }),
  bulkExportCancel: () => invoke("bulk_export_cancel"),
  onBulkProgress: (cb) => listen<BulkProgress>("bulk-export-progress", (e) => cb(e.payload)),
  pickReportPath: (name) => save({ defaultPath: `${name}.html`, filters: [{ name: "Report", extensions: ["html"] }] }),
  pickBulkPath: (name, format) =>
    format === "xlsx"
      ? save({ defaultPath: `${name}.xlsx`, filters: [{ name: "Excel workbook", extensions: ["xlsx"] }] })
      : save({ defaultPath: name, title: "Folder for the CSV files" }),
};
