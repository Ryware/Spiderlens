// In-memory link metrics and Google Sheets export (spec 024 US5 and US6) for
// UI tests. The providers and the Sheets API are tested in Rust against
// fakes (crates/integrations/tests/links.rs and sheets.rs).

import type { LinkEstimate } from "../generated/LinkEstimate";
import type { LinkRequest } from "../generated/LinkRequest";
import type { MetricInfo } from "../generated/MetricInfo";
import type { Overflow } from "../generated/Overflow";
import type { RunProgress } from "../generated/RunProgress";
import type { SheetExport } from "../generated/SheetExport";
import type { SheetsPlan } from "../generated/SheetsPlan";
import type { SpreadsheetRef } from "../generated/SpreadsheetRef";
import type { Transport } from "../transport";
import type { FakeIntegrations } from "./fakeIntegrations";

type Links = Pick<
  Transport,
  | "integrationsLinkMetrics"
  | "integrationsLinkEstimate"
  | "integrationsRunLinks"
  | "integrationsSheetsPlan"
  | "integrationsExportSheets"
  | "integrationsOpenLooker"
  | "integrationsLookerLink"
  | "onSheetsProgress"
>;

export interface FakeLinks {
  /** Pages the estimate covers, and how many of them are cached. */
  pages: number;
  cached: number;
  /** The provider's units one uncached page costs. */
  unitsPerPage: number;
  /** Estimates asked for, and runs started, in order. */
  estimates: LinkRequest[];
  runs: LinkRequest[];
  /** Data rows the view being exported has, and the cell limit of one spreadsheet. */
  sheetRows: number;
  capacity: number;
  /** What each export asked for. */
  exports: { title: string; worksheet: string; columns: string[]; overflow: Overflow }[];
  /** Links opened in the browser. */
  opened: string[];
}

const METRICS: Record<string, MetricInfo[]> = {
  ahrefs: [
    { id: "domain_rating", label: "Domain Rating" },
    { id: "url_rating", label: "URL Rating" },
    { id: "backlinks", label: "Backlinks" },
    { id: "refdomains", label: "Referring domains" },
  ],
  majestic: [
    { id: "trust_flow", label: "Trust Flow" },
    { id: "citation_flow", label: "Citation Flow" },
    { id: "backlinks", label: "Backlinks" },
    { id: "refdomains", label: "Referring domains" },
  ],
  moz: [
    { id: "page_authority", label: "Page Authority" },
    { id: "domain_authority", label: "Domain Authority" },
    { id: "linking_domains", label: "Linking domains" },
    { id: "backlinks", label: "Backlinks" },
    { id: "spam_score", label: "Spam Score" },
  ],
};
const UNITS: Record<string, string> = { ahrefs: "API units", majestic: "units", moz: "rows" };

export function fakeLinksTransport(fi: FakeIntegrations): { api: Links; linkMetrics: FakeLinks } {
  const fl: FakeLinks = {
    pages: 120,
    cached: 0,
    unitsPerPage: 4,
    estimates: [],
    runs: [],
    sheetRows: 1200,
    capacity: 10_000_000,
    exports: [],
    opened: [],
  };
  const listeners = new Set<(p: { sent: number; total: number }) => void>();

  const estimate = (r: LinkRequest): LinkEstimate => {
    const units = Math.max(0, fl.pages - fl.cached) * fl.unitsPerPage;
    const default_cap = units + Math.ceil(units / 10);
    return {
      provider: r.provider,
      unit: UNITS[r.provider] ?? "units",
      urls: fl.pages,
      cached: fl.cached,
      requests: Math.ceil((fl.pages - fl.cached) / 100),
      units,
      default_cap,
      cap: r.cap ?? default_cap,
    };
  };

  const plan = (columns: number): SheetsPlan => {
    const cols = Math.max(1, columns);
    const per = Math.max(0, Math.floor(fl.capacity / cols) - 1);
    const cells = (fl.sheetRows + 1) * cols;
    return {
      rows: fl.sheetRows,
      columns: cols,
      cells,
      capacity: fl.capacity,
      fits: cells <= fl.capacity,
      rows_per_spreadsheet: per,
      spreadsheets: per === 0 ? 0 : Math.max(1, Math.ceil(fl.sheetRows / per)),
    };
  };

  const sheet = (n: number, title: string, rows: number): SpreadsheetRef => ({
    spreadsheet_id: `fake-sheet-${n}`,
    url: `https://docs.google.com/spreadsheets/d/fake-sheet-${n}/edit`,
    worksheet_id: 0,
    title,
    rows,
  });
  const looker = (s: SpreadsheetRef) =>
    `https://lookerstudio.google.com/reporting/create?c.reportId=&ds.ds0.connector=googleSheets&ds.ds0.spreadsheetId=${s.spreadsheet_id}&ds.ds0.worksheetId=${s.worksheet_id}`;

  const api: Links = {
    integrationsLinkMetrics: async (provider) => structuredClone(METRICS[provider] ?? []),
    integrationsLinkEstimate: async (_path, _label, request) => {
      fl.estimates.push(request);
      return estimate(request);
    },
    integrationsRunLinks: async (_path, label, request) => {
      fl.runs.push(request);
      const e = estimate(request);
      const room = Math.floor(e.cap / fl.unitsPerPage);
      const need = fl.pages - fl.cached;
      const fetched = Math.min(need, room);
      const done = fl.cached + fetched;
      const r: RunProgress = {
        id: (fi.runs[0]?.id ?? 0) + 1,
        provider: request.provider,
        account_label: label,
        property: null,
        params: {},
        state: "finished",
        requested: fl.pages,
        done,
        failed: 0,
        matched: done,
        unmatched: 0,
        usage: fetched * fl.unitsPerPage,
        resume_at: null,
        message: done < fl.pages ? `Stopped at the cap of ${e.cap} ${e.unit}.` : null,
        created_at: "2026-10-14T08:00:00Z",
        finished_at: "2026-10-14T08:01:00Z",
      };
      fi.runs.unshift(r);
      return { ...r };
    },
    integrationsSheetsPlan: async (_path, _query, columns) => plan(columns.length),
    integrationsExportSheets: async (_path, _query, columns, _segments, title, worksheet, _account, overflow) => {
      fl.exports.push({ title, worksheet, columns, overflow });
      const p = plan(columns.length);
      if (!p.fits && overflow === "refuse") {
        throw new Error(p.rows_per_spreadsheet ? "Too many cells for one spreadsheet." : "Too many columns.");
      }
      const tick = (sent: number) => listeners.forEach((cb) => cb({ sent, total: fl.sheetRows }));
      if (p.fits) {
        tick(fl.sheetRows);
        const out: SheetExport = { spreadsheets: [sheet(1, title, fl.sheetRows)], rows: fl.sheetRows, total_rows: fl.sheetRows, truncated: false };
        return out;
      }
      if (overflow === "truncate") {
        tick(p.rows_per_spreadsheet);
        return { spreadsheets: [sheet(1, title, p.rows_per_spreadsheet)], rows: p.rows_per_spreadsheet, total_rows: fl.sheetRows, truncated: true };
      }
      const parts = Array.from({ length: p.spreadsheets }, (_, i) =>
        sheet(i + 1, `${title} (part ${i + 1} of ${p.spreadsheets})`, Math.min(p.rows_per_spreadsheet, fl.sheetRows - i * p.rows_per_spreadsheet)),
      );
      tick(fl.sheetRows);
      return { spreadsheets: parts, rows: fl.sheetRows, total_rows: fl.sheetRows, truncated: false };
    },
    integrationsOpenLooker: async (s) => {
      const link = looker(s);
      fl.opened.push(link);
      return link;
    },
    integrationsLookerLink: async (s) => looker(s),
    onSheetsProgress: async (cb) => {
      listeners.add(cb);
      return () => void listeners.delete(cb);
    },
  };
  return { api, linkMetrics: fl };
}
