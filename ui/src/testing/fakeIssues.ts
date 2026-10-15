// In-memory issue rules for UI tests (spec 009): two rules over the fake
// URLs. The real catalogue and evaluation are tested in Rust
// (crates/results/tests/rules.rs).

import type { IssueSettings } from "../generated/IssueSettings";
import type { IssuesSummary } from "../generated/IssuesSummary";
import type { IssuePriority as Priority } from "../generated/IssuePriority";
import type { RuleInfo } from "../generated/RuleInfo";
import type { SettingsNotice } from "../generated/SettingsNotice";
import type { TabDef } from "../generated/TabDef";
import type { Transport } from "../transport";
import type { FakeUrl } from "./fakeResults";

const base: RuleInfo[] = [
  {
    id: "response.internal_4xx",
    name: "Internal client error (4xx)",
    category: "response_codes",
    category_label: "Response codes",
    priority: "high",
    default_priority: "high",
    enabled: true,
    kind: "per_url",
    scope: "Internal URLs",
    threshold: null,
    standards: [],
    why: "Visitors and crawlers following these links reach an error page.",
    fix: "Restore the page, redirect it, or fix the links to it.",
  },
  {
    id: "title.below",
    name: "Page title: below 30 characters",
    category: "page_titles",
    category_label: "Page titles",
    priority: "medium",
    default_priority: "medium",
    enabled: true,
    kind: "per_url",
    scope: "HTML pages",
    threshold: { unit: "chars", unit_label: "characters", value: 30, default: 30, min: 1, max: 1000 },
    standards: [],
    why: "Short titles waste room in search results.",
    fix: "Describe the page in a longer title.",
  },
];

/** The rules with `s` applied, and what is wrong with it. */
export function applySettings(s: IssueSettings): { rules: RuleInfo[]; notices: SettingsNotice[] } {
  const notices: SettingsNotice[] = [];
  const rules = base.map((r) => {
    const o = s.rules[r.id];
    if (!o) return r;
    const out = { ...r, enabled: o.enabled ?? true, priority: (o.priority ?? r.default_priority) as Priority };
    if (r.threshold && o.threshold !== null && o.threshold !== undefined) {
      if (o.threshold < r.threshold.min || o.threshold > r.threshold.max) {
        notices.push({ rule: r.id, error: true, message: `must be between ${r.threshold.min} and ${r.threshold.max}` });
      } else {
        out.threshold = { ...r.threshold, value: o.threshold };
        out.name = `Page title: below ${o.threshold} characters`;
      }
    }
    return out;
  });
  for (const id of Object.keys(s.rules)) if (!base.some((r) => r.id === id)) notices.push({ rule: id, error: false, message: "unknown rule, ignored" });
  return { rules: notices.some((n) => n.error) ? base : rules, notices };
}

export const fakeIssueTabs: TabDef[] = base.map((r) => ({
  id: `issue:${r.id}`,
  title: r.name,
  columns: [
    { id: "address", label: "Address", kind: "url", width: 420, default_visible: true, need: "base" },
    { id: "status_code", label: "Status code", kind: "status", width: 96, default_visible: true, need: "base" },
  ],
  filters: [{ id: "all", label: "All", need: "base" }],
  search_column: null,
  scoped: false,
}));

/** URLs with this rule's issue, given the rules in force. */
export function hits(rule: RuleInfo, urls: FakeUrl[]): FakeUrl[] {
  if (!rule.enabled) return [];
  if (rule.id === "response.internal_4xx") return urls.filter((u) => u.internal && u.status !== null && u.status >= 400 && u.status < 500);
  const limit = rule.threshold?.value ?? 30;
  return urls.filter((u) => u.title !== null && u.title.length < limit);
}

export interface FakeIssues {
  /** Settings last applied. */
  settings: IssueSettings;
  rules(): RuleInfo[];
  /** While true, issuesSummary waits for release(), as a long evaluation does. */
  hold: boolean;
  release(): void;
  /** Sends the issues-partial event with the current counts marked provisional. */
  partial(path: string): void;
}

export function fakeIssuesTransport(urls: () => FakeUrl[]): Pick<Transport, "ruleCatalogue" | "issuesApply" | "issuesSummary" | "onIssuesPartial" | "urlIssues"> & { issues: FakeIssues } {
  let version = 1;
  const listeners = new Set<(p: { path: string; summary: IssuesSummary }) => void>();
  let waiting: (() => void)[] = [];
  const issues: FakeIssues = {
    settings: { rules: {} },
    rules: () => applySettings(issues.settings).rules,
    hold: false,
    release: () => {
      issues.hold = false;
      waiting.forEach((go) => go());
      waiting = [];
    },
    partial: (path) => {
      const s = summarize(false);
      const summary = { ...s, issues: s.issues.map((i) => ({ ...i, provisional: true })) };
      listeners.forEach((cb) => cb({ path, summary }));
    },
  };
  const summarize = (running: boolean): IssuesSummary => {
    const rules = issues.rules();
    const counts = rules.map((r) => ({
      id: r.id,
      name: r.name,
      category: r.category,
      priority: r.priority,
      enabled: r.enabled,
      count: r.enabled ? hits(r, urls()).length : null,
      provisional: running && r.kind === "site_wide",
      unavailable: null,
    }));
    const order: Priority[] = ["high", "medium", "low", "opportunity"];
    return {
      version,
      rules_version: 1,
      by_priority: order.map((p) => {
        const at = counts.filter((c) => c.priority === p && (c.count ?? 0) > 0);
        return { priority: p, issues: at.length, urls: at.reduce((n, c) => n + (c.count ?? 0), 0) };
      }),
      issues: counts,
    };
  };
  return {
    issues,
    ruleCatalogue: async (s) => applySettings(s),
    issuesApply: async (_path, s) => {
      const { notices } = applySettings(s);
      if (!notices.some((n) => n.error)) {
        issues.settings = s;
        version++;
      }
      return notices;
    },
    issuesSummary: async (_path, running): Promise<IssuesSummary> => {
      if (issues.hold) await new Promise<void>((go) => waiting.push(go));
      return summarize(running);
    },
    onIssuesPartial: async (cb) => {
      listeners.add(cb);
      return () => void listeners.delete(cb);
    },
    urlIssues: async (_path, id) => {
      const u = urls().find((x) => x.id === id);
      if (!u) return [];
      return issues
        .rules()
        .filter((r) => hits(r, [u]).length > 0)
        .map((r) => ({ id: r.id, name: r.name, priority: r.priority, evidence: r.id === "title.below" ? `${u.title?.length} characters` : `${u.status}`, targets: [], targets_total: 0 }));
    },
  };
}
