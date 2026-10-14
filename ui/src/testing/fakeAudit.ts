// In-memory report and bulk export commands for UI tests (spec 010). The
// report itself is built and tested in Rust (crates/results/tests/report.rs);
// this is a fixed sample with every block kind.

import type { BulkOptions } from "../generated/BulkOptions";
import type { BulkProgress } from "../generated/BulkProgress";
import type { BulkResult } from "../generated/BulkResult";
import type { FileOptions } from "../generated/FileOptions";
import type { Report } from "../generated/Report";
import { EXISTS, type Transport } from "../transport";

export function sampleReport(over: Partial<Report> = {}): Report {
  return {
    meta: {
      start_url: "https://example.com/",
      status: "finished",
      active_ms: 83_000,
      profile: null,
      app_version: "0.1.0",
      rules_version: 1,
      score_version: 1,
      custom_issue_settings: false,
    },
    provisional: false,
    incomplete: null,
    score: {
      value: 72,
      unavailable: null,
      partial: false,
      pages: 25,
      by_priority: [
        { priority: "high", points: 20, pages: 5 },
        { priority: "medium", points: 8, pages: 4 },
        { priority: "low", points: 0, pages: 0 },
        { priority: "opportunity", points: 0, pages: 0 },
      ],
      issues: [
        { id: "title_missing", name: "Missing page title", priority: "high", pages: 5, share: 20, gain: 20 },
        { id: "meta_description_missing", name: "Missing meta description", priority: "medium", pages: 4, share: 8, gain: 8 },
      ],
      not_run: [],
    },
    sections: [
      {
        id: "summary",
        title: "Summary",
        state: { kind: "available" },
        blocks: [
          {
            kind: "figures",
            items: [
              { label: "Internal URLs", value: "30", target: { kind: "tab", tab: "internal", filter: "all", scope: "all" } },
              { label: "Client errors (4xx)", value: "2", target: { kind: "tab", tab: "response-codes", filter: "client_error", scope: "internal" } },
              { label: "Crawl time", value: "1 min 23 s", target: null },
            ],
          },
        ],
      },
      {
        id: "issues",
        title: "Issues",
        state: { kind: "available" },
        blocks: [
          {
            kind: "issue",
            issue: {
              id: "title_missing",
              name: "Missing page title",
              category: "Page titles",
              priority: "high",
              count: 5,
              provisional: false,
              unavailable: null,
              why: "Search results show the title as the link text.",
              fix: "Give every page a unique title.",
              gain: 20,
            },
          },
        ],
      },
      {
        id: "structure",
        title: "Site structure",
        state: { kind: "available" },
        blocks: [
          {
            kind: "bars",
            items: [
              { label: "Depth 0", value: 1, share: 0.05, target: null },
              { label: "Depth 1", value: 20, share: 1, target: null },
            ],
          },
          { kind: "table", columns: ["URL", "Depth"], rows: [[{ text: "https://example.com/deep", target: null }, { text: "3", target: null }]] },
        ],
      },
      { id: "content", title: "Content", state: { kind: "unavailable", reason: "This crawl did not store page content." }, blocks: [] },
    ],
    ...over,
  };
}

/** What a test can see of report commands and how it drives bulk exports. */
export interface FakeAudit {
  report: Report;
  asked: { path: string; running: boolean }[];
  written: { dest: string; file: FileOptions; overwrite: boolean }[];
  printed: FileOptions[];
  bulk: BulkOptions[];
  /** Paths that already exist: writing them without overwrite fails. */
  existing: Set<string>;
  progress(p: BulkProgress): void;
  finishBulk(r?: Partial<BulkResult>): void;
  failBulk(message: string): void;
}

export function fakeAuditTransport(): Pick<
  Transport,
  "report" | "exportReportHtml" | "printReport" | "bulkExport" | "bulkExportCancel" | "onBulkProgress" | "pickReportPath" | "pickBulkPath"
> & { audit: FakeAudit } {
  const listeners = new Set<(p: BulkProgress) => void>();
  let pending: { resolve: (r: BulkResult) => void; reject: (e: unknown) => void } | null = null;
  const audit: FakeAudit = {
    report: sampleReport(),
    asked: [],
    written: [],
    printed: [],
    bulk: [],
    existing: new Set(),
    progress: (p) => listeners.forEach((cb) => cb(p)),
    finishBulk: (r) => {
      pending?.resolve({ files: ["/exports/example.com.xlsx"], tables: [{ table: "Internal", rows: 30 }], truncated_cells: 0, split_sheets: 0, ...r });
      pending = null;
    },
    failBulk: (message) => {
      pending?.reject(message);
      pending = null;
    },
  };
  return {
    audit,
    report: async (path, running) => {
      audit.asked.push({ path, running });
      return structuredClone(audit.report);
    },
    exportReportHtml: async (_path, dest, file, overwrite) => {
      if (audit.existing.has(dest) && !overwrite) throw EXISTS;
      audit.written.push({ dest, file, overwrite });
      return { path: dest, bytes: 1234 };
    },
    printReport: async (_path, file) => void audit.printed.push(file),
    bulkExport: (_path, options) => {
      if (audit.existing.has(options.out) && !options.overwrite) return Promise.reject(EXISTS);
      audit.bulk.push(options);
      return new Promise((resolve, reject) => (pending = { resolve, reject }));
    },
    bulkExportCancel: async () => audit.failBulk("export cancelled"),
    onBulkProgress: async (cb) => {
      listeners.add(cb);
      return () => void listeners.delete(cb);
    },
    pickReportPath: async (name) => `/exports/${name}.html`,
    pickBulkPath: async (name, format) => (format === "xlsx" ? `/exports/${name}.xlsx` : `/exports/${name}`),
  };
}
