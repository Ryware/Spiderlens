// In-memory custom extraction and search for UI tests (spec 013). Rules
// "run" by simple text matching on each fake page's HTML; the real engine is
// tested in Rust (crates/engine/src/custom, crates/results/tests/custom.rs).

import type { CustomResult } from "../generated/CustomResult";
import type { CustomRules } from "../generated/CustomRules";
import type { CrawlRule, CustomRunDone, CustomRunProgress, RuleTestOutcome, RuleTestResult, Transport } from "../transport";

export interface FakeCustom {
  /** Each page's HTML by URL id, for tests and runs. */
  html: Map<number, string>;
  runs: { path: string; rules: CustomRules | null; only: string[] }[];
  cancelled: number[];
  tested: { url?: string; urlId?: number; rules: CustomRules }[];
  imported: CustomRules;
  exportedTo: { path: string; rules: CustomRules }[];
  /** Rules stored with the open crawl. */
  stored: CrawlRule[];
  /** Per page id: results the detail pane shows. */
  results: Map<number, CustomResult[]>;
  /** A typed URL that fails to load, for error paths. */
  failingUrl: string | null;
  progress(p: Partial<CustomRunProgress>): void;
  done(d: Partial<CustomRunDone>): void;
}

/** What one rule finds in `html`: a plain-text stand-in for the engine. */
export function fakeApply(rules: CustomRules, html: string): RuleTestResult[] {
  const out: RuleTestResult[] = [];
  for (const e of rules.extractors) {
    // `.class` and `tag` selectors on simple markup; anything else matches nothing.
    const cls = e.expr.startsWith(".") ? e.expr.slice(1) : null;
    const re = cls ? new RegExp(`class="${cls}"[^>]*>([^<]*)<`, "g") : new RegExp(`<${e.expr}[^>]*>([^<]*)<`, "g");
    const values = [...html.matchAll(re)].map((m) => m[1]);
    out.push({ name: e.name, kind: "extractor", count: values.length, flags: values.length ? [] : ["no_match"], values, error: null });
  }
  for (const s of rules.searches) {
    const hay = s.ignore_case ? html.toLowerCase() : html;
    const term = s.ignore_case ? s.term.toLowerCase() : s.term;
    const count = term ? hay.split(term).length - 1 : 0;
    const meets = (count > 0) !== (s.condition === "not_contains");
    out.push({ name: s.name, kind: "search", count, flags: [...(meets ? ["meets"] : []), ...(count ? [] : ["no_match"])], values: count ? [`…${s.term}…`] : [], error: null });
  }
  return out;
}

export function fakeCustomTransport(): Pick<
  Transport,
  | "urlCustom"
  | "customTestUrl"
  | "customTestPage"
  | "customRulesOfCrawl"
  | "customRun"
  | "customCancel"
  | "onCustomRunProgress"
  | "onCustomRunDone"
  | "pickRulesFile"
  | "pickRulesSavePath"
  | "customImport"
  | "customExport"
> & { custom: FakeCustom } {
  const progressCbs = new Set<(p: CustomRunProgress) => void>();
  const doneCbs = new Set<(d: CustomRunDone) => void>();
  let next = 0;
  const custom: FakeCustom = {
    html: new Map(),
    runs: [],
    cancelled: [],
    tested: [],
    imported: {
      extractors: [{ name: "Imported price", kind: "css", expr: ".price", mode: "text" }],
      searches: [],
    },
    exportedTo: [],
    stored: [],
    results: new Map(),
    failingUrl: null,
    progress: (p) => progressCbs.forEach((cb) => cb({ run: next, done: 0, total: 0, per_sec: 0, ...p })),
    done: (d) => doneCbs.forEach((cb) => cb({ run: next, path: "/crawls/example.com.spiderlens", state: "complete", pages: 0, error: null, ...d })),
  };
  const outcome = (url: string, status: number | null, rules: CustomRules, html: string): RuleTestOutcome => ({
    url,
    status,
    results: fakeApply(rules, html),
    ms: 3,
  });
  return {
    custom,
    urlCustom: async (_p, id) => custom.results.get(id) ?? [],
    customTestUrl: async (rules, url) => {
      custom.tested.push({ url, rules });
      if (url === custom.failingUrl) throw "The server did not answer.";
      return outcome(url, 200, rules, custom.html.get(1) ?? `<span class="price">$10</span><p>consent-v2.js</p>`);
    },
    customTestPage: async (rules, _path, urlId) => {
      custom.tested.push({ urlId, rules });
      const html = custom.html.get(urlId);
      if (html === undefined) throw "This crawl did not store this page's HTML, so it can only be tested by URL.";
      return outcome(`https://example.com/page-${urlId}`, null, rules, html);
    },
    customRulesOfCrawl: async () => custom.stored,
    customRun: async (path, rules, only) => {
      custom.runs.push({ path, rules, only: only ?? [] });
      return ++next;
    },
    customCancel: async (run) => void custom.cancelled.push(run),
    onCustomRunProgress: async (cb) => {
      progressCbs.add(cb);
      return () => void progressCbs.delete(cb);
    },
    onCustomRunDone: async (cb) => {
      doneCbs.add(cb);
      return () => void doneCbs.delete(cb);
    },
    pickRulesFile: async () => "/rules/shop.spiderlens-rules.json",
    pickRulesSavePath: async (name) => `/rules/${name}.spiderlens-rules.json`,
    customImport: async () => structuredClone(custom.imported),
    customExport: async (path, rules) => void custom.exportedTo.push({ path, rules: structuredClone(rules) }),
  };
}
