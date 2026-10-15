// In-memory AI checks (spec 015) for UI tests. The analysis itself is tested
// in Rust (crates/aeo/tests); this only plays back results and events.

import type { AeoRule } from "../generated/AeoRule";
import type { AiPageResult } from "../generated/AiPageResult";
import type { AiProgress } from "../generated/AiProgress";
import type { AiSummary } from "../generated/AiSummary";
import type { Part } from "../generated/Part";
import type { AeoFinished, AeoProgress, Transport } from "../transport";

const rule = (id: string, name: string, part: Part, points: number, how_to_fix: string): AeoRule => ({
  rule: { id, name, category: "AI search", priority: "Medium", kind: part === "site" ? "site_wide" : "per_page", needs: ["base"], explanation: `${name}.`, how_to_fix },
  part,
  points,
  applies_to: part === "site" ? "site" : "all_pages",
  basis: { kind: "common_practice" },
  flag: null,
});

export const fakeRules: AeoRule[] = [
  rule("ai.access.search_blocked", "AI search crawlers are blocked", "access", 15, "Allow the search crawlers in robots.txt."),
  rule("ai.readable.js_suspected", "Content needs JavaScript", "readable", 15, "Serve the main text in the HTML."),
  rule("ai.trust.no_author", "No author", "trust", 5, "Name the author on the page and in structured data."),
  rule("ai.structured.errors", "Structured data has errors", "structured", 5, "Fix the errors the parser reports."),
  rule("ai.site.llms_txt", "No llms.txt", "site", 5, "Publish /llms.txt with a summary and key links."),
];

export function fakeSummary(patch: Partial<AiSummary> = {}): AiSummary {
  return {
    score: 72,
    pages_score: 70,
    site_checks: 77,
    parts: [
      { part: "access", score: 27, max: 30 },
      { part: "readable", score: 18, max: 25 },
      { part: "answer", score: 14, max: 20 },
      { part: "trust", score: 8, max: 15 },
      { part: "structured", score: null, max: 10 },
    ],
    distribution: [1, 3, 10, 16],
    top_fixes: [
      { rule_id: "ai.trust.no_author", pages: 12, gain: 420 },
      { rule_id: "ai.site.llms_txt", pages: 0, gain: 150 },
    ],
    site_outcomes: [{ rule_id: "ai.site.llms_txt", outcome: "fail", evidence: "/llms.txt returned 404" }],
    hosts: [
      {
        host_key: "https://example.com",
        robots: "ok",
        access: [
          { bot: "gptbot", home_verdict: "blocked", matched_group: "GPTBot", urls_total: 30, urls_blocked: 30, rules: [{ line_no: 4, rule: "Disallow: /", urls: 30 }] },
          { bot: "oai-searchbot", home_verdict: "allowed", matched_group: null, urls_total: 30, urls_blocked: 0, rules: [] },
        ],
        signals: [],
        llms_txt: { outcome: "not_found", report: null },
      },
    ],
    entity: { kind: "Organization", name: "Example", url: "https://example.com/", logo: null, same_as: [], contact: false },
    about_url: "https://example.com/about",
    contact_url: null,
    pages_analyzed: 30,
    provisional_pages: 0,
    versions: { analysis: 1, app: "0.1.0", bots: 1, rules: "1" },
    ...patch,
  };
}

export function fakePageResult(url: string, patch: Partial<AiPageResult> = {}): AiPageResult {
  return {
    url,
    page_type: "article",
    page_type_reason: "BlogPosting structured data",
    score: 64,
    provisional: false,
    version_used: "1",
    published: "2026-09-01",
    modified: null,
    author: null,
    blocked_crawlers: ["GPTBot"],
    checks: [
      { rule_id: "ai.trust.no_author", outcome: "fail", points_lost: 500, evidence: null },
      { rule_id: "ai.structured.errors", outcome: "not_available", points_lost: 0, evidence: null },
    ],
    ...patch,
  };
}

export interface FakeAeo {
  /** Stored results of the open file; null until the checks run. */
  summary: AiSummary | null;
  /** Per URL id; missing ids have no result. */
  pages: Map<number, AiPageResult>;
  runs: string[];
  cancelled: number[];
  progress(p: AiProgress): void;
  /** Ends the latest run; stores the summary unless `summary` is null. */
  finish(f?: Partial<AeoFinished>): void;
}

type Api = Pick<Transport, "aeoRun" | "aeoCancel" | "aeoSummary" | "aeoPage" | "aeoLlmsTxtDraft" | "aeoRules" | "aiBotsList" | "onAeoProgress" | "onAeoFinished">;

export function fakeAeoTransport(): Api & { aeo: FakeAeo } {
  const progressCbs = new Set<(p: AeoProgress) => void>();
  const finishedCbs = new Set<(f: AeoFinished) => void>();
  let runId = 0;
  let path = "";
  const aeo: FakeAeo = {
    summary: null,
    pages: new Map(),
    runs: [],
    cancelled: [],
    progress: (p) => progressCbs.forEach((cb) => cb({ ...p, run_id: runId, path })),
    finish: (f = {}) => {
      const done: AeoFinished = { run_id: runId, path, summary: fakeSummary(), error: null, ...f };
      if (done.summary) aeo.summary = done.summary;
      finishedCbs.forEach((cb) => cb(done));
    },
  };
  return {
    aeo,
    aeoRun: async (p) => {
      path = p;
      aeo.runs.push(p);
      return { run_id: ++runId };
    },
    aeoCancel: async (id) => void aeo.cancelled.push(id),
    aeoSummary: async () => aeo.summary,
    aeoPage: async (_p, id) => aeo.pages.get(id) ?? null,
    aeoLlmsTxtDraft: async () => ({ text: "# Example\n\n> A site.\n" }),
    aeoRules: async () => fakeRules,
    aiBotsList: async () => ({ version: 1, date: "2026-10-01", bots: [] }),
    onAeoProgress: async (cb) => {
      progressCbs.add(cb);
      return () => void progressCbs.delete(cb);
    },
    onAeoFinished: async (cb) => {
      finishedCbs.add(cb);
      return () => void finishedCbs.delete(cb);
    },
  };
}
