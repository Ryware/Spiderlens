// In-memory AI agent findings and assistant settings for UI tests (spec 016).
// The real collection, storage and MCP server are tested in Rust
// (crates/engine/tests/agents.rs, crates/results/tests/agents.rs,
// crates/mcp/tests/server.rs).

import type { ActivityEntry } from "../generated/ActivityEntry";
import type { AssistantProfile } from "../generated/AssistantProfile";
import type { AssistantSettings } from "../generated/AssistantSettings";
import type { AgentsSummary } from "../generated/AgentsSummary";
import type { EntryPointRow } from "../generated/EntryPointRow";
import type { PageAgentFindings } from "../generated/PageAgentFindings";
import type { Need } from "../generated/Need";
import type { McpSelfTest, Transport } from "../transport";
import type { FakeUrl } from "./fakeResults";

export const fakeExecutable = "/Applications/SpiderLens.app/Contents/MacOS/spiderlens-desktop";

/** What a test can see of Settings › AI assistants and how it changes it. */
export interface FakeAssistants {
  settings: AssistantSettings;
  activity: ActivityEntry[];
  /** Folders the folder picker returns, in order. */
  folders: string[];
  /** Folders that "do not exist" when saved. */
  missing: Set<string>;
  /** Set to make the self-test fail with this message. */
  testError: string | null;
  saves: AssistantSettings[];
  /** Saved profiles offered for crawling. */
  profiles: AssistantProfile[];
}

export function fakeActivity(n: number): ActivityEntry[] {
  return Array.from({ length: n }, (_, i) => ({
    at: `2026-10-14T05:${String(59 - i).padStart(2, "0")}:00Z`,
    client: "claude-code 2.4",
    tool: i % 3 === 0 ? "query_pages" : i % 3 === 1 ? "page_details" : "list_crawls",
    crawl: i % 3 === 2 ? null : "c_0a1b2c3d",
    bytes: 1024 * (i + 1),
    refused: i === 1,
    error: i === 1 ? "refused" : null,
  }));
}

/** Findings for a fake URL: id 2 has a contact form with an unlabelled
 * field, a nameless button and a fake link; the rest are plain pages. */
export function fakeFindings(u: FakeUrl, features: Need[]): PageAgentFindings {
  const base: PageAgentFindings = {
    id: u.id,
    url: u.url,
    available: features.includes("agents"),
    unavailable_reason: null,
    checked: false,
    forms: [],
    loose_fields: [],
    unlabelled_fields: 0,
    placeholder_only_fields: 0,
    nameless_buttons: 0,
    actions: [],
    actions_total: 0,
    links: [],
  };
  if (!base.available) return { ...base, unavailable_reason: "Forms and actions were not collected in this crawl file; it was made before SpiderLens checked agent readiness." };
  if (!u.internal || u.status !== 200) return base;
  if (u.id !== 2) return { ...base, checked: true };
  return {
    ...base,
    checked: true,
    forms: [
      {
        source: "raw",
        method: "post",
        action: "https://example.com/contact",
        is_search: false,
        has_submit: true,
        challenge: "recaptcha",
        tool_name: "contact_us",
        tool_description: null,
        tool_autosubmit: false,
        fields: [
          { name: "email", kind: "email", required: true, accessible_name: "Email", name_source: "label", description: null, tool_param_description: null },
          { name: "phone", kind: "tel", required: false, accessible_name: "Phone", name_source: "placeholder", description: null, tool_param_description: null },
          { name: "msg", kind: "textarea", required: false, accessible_name: null, name_source: "none", description: null, tool_param_description: null },
        ],
      },
    ],
    unlabelled_fields: 1,
    placeholder_only_fields: 1,
    nameless_buttons: 1,
    actions: [{ source: "raw", kind: "onclick", text: "Open menu" }],
    actions_total: 1,
    links: [{ source: "raw", kind: "markdown_alternate", href: "https://example.com/contact.md", via: "html" }],
  };
}

/** A scored crawl: 72 overall, three failed rules and two checks that could not run. */
export const fakeAgentsSummary: AgentsSummary = {
  available: true,
  unavailable_reason: null,
  score: 72,
  parts: [
    { part: "access", weight: 30, score: 90 },
    { part: "discovery", weight: 15, score: 40 },
    { part: "reading", weight: 25, score: 75 },
    { part: "acting", weight: 30, score: 62.5 },
  ],
  pages_scored: 120,
  top_failed: [
    { id: "agent-form-fields-unlabelled", name: "Form fields without a label", part: "acting", site_wide: false, pages: 14, points_lost: 6.4 },
    { id: "agent-no-ai-catalog", name: "No AI Catalog", part: "discovery", site_wide: true, pages: null, points_lost: 4.5 },
    { id: "agent-no-markdown", name: "No Markdown version", part: "reading", site_wide: false, pages: 96, points_lost: 3 },
  ],
  not_checked: [
    { id: "agent-challenge-pages", name: "Challenge pages", reason: "No page was rendered, so SpiderLens could not see a CAPTCHA." },
    { id: "agent-markdown-negotiation", name: "Markdown by content negotiation", reason: "Turned off in this crawl's settings." },
  ],
  weights_version: 1,
  access: [
    { bot: "ChatGPT-User", vendor: "OpenAI", purpose: "user_fetch", robots: "allowed", probe: "ok" },
    { bot: "Claude-User", vendor: "Anthropic", purpose: "user_fetch", robots: "blocked", probe: "not_probed" },
    { bot: "Perplexity-User", vendor: "Perplexity", purpose: "user_fetch", robots: "allowed", probe: "challenged" },
    { bot: "Meta-ExternalFetcher", vendor: "Meta", purpose: "user_fetch", robots: "not_checked", probe: "blocked" },
  ],
};

export const fakeAgentsUnavailable: AgentsSummary = {
  available: false,
  unavailable_reason: "Agent checks have not run on this crawl.",
  score: null,
  parts: [],
  pages_scored: 0,
  top_failed: [],
  not_checked: [],
  weights_version: null,
  access: [],
};

const wellKnown = { ai_catalog: "ai-catalog.json", api_catalog: "api-catalog", a2a_card: "agent-card.json" } as const;

const entryPoint = (id: number, kind: keyof typeof wellKnown, outcome: string, status: number | null, extra: Partial<EntryPointRow> = {}): EntryPointRow => ({
  id,
  host_key: "example.com",
  kind,
  url: `https://example.com/.well-known/${wellKnown[kind]}`,
  found_via: "well_known",
  found_on: null,
  status,
  outcome,
  content_type: null,
  size: null,
  convention_version: null,
  problems: [],
  summary: null,
  mcp_discovery: null,
  ...extra,
});

/** One working AI Catalog, no agent card and an API catalog that is not valid. */
export const fakeEntryPoints: EntryPointRow[] = [
  entryPoint(1, "ai_catalog", "ok", 200, { content_type: "application/json", size: 812 }),
  entryPoint(2, "a2a_card", "absent", 404),
  entryPoint(3, "api_catalog", "invalid", 200, {
    content_type: "application/linkset+json",
    problems: [{ code: "missing_linkset", message: "The API catalog has no `linkset` list." }],
  }),
];

export function fakeAgentsTransport(
  urls: () => FakeUrl[],
  features: () => Need[],
): Pick<
  Transport,
  "agentsPage" | "agentsSummary" | "agentsEntryPoints" | "assistantsGet" | "assistantsSet" | "assistantsTest" | "assistantsActivity" | "assistantsActivityClear" | "pickFolder"
> & { assistants: FakeAssistants } {
  const assistants: FakeAssistants = {
    settings: { allow_read: false, allowed_folders: [], allow_crawl: false, allowed_profiles: [] },
    activity: [],
    folders: ["/Users/me/Crawls"],
    missing: new Set(),
    testError: null,
    saves: [],
    profiles: [
      { name: "Polite", respects_robots: true, reason: null },
      { name: "Stress test", respects_robots: false, reason: "Ignores robots.txt" },
    ],
  };
  const config = JSON.stringify({ mcpServers: { spiderlens: { command: fakeExecutable, args: ["mcp"] } } }, null, 2);
  return {
    assistants,
    agentsPage: async (_p, id) => fakeFindings(urls().find((u) => u.id === id)!, features()),
    agentsSummary: async () => (features().includes("agents") ? fakeAgentsSummary : fakeAgentsUnavailable),
    agentsEntryPoints: async () => (features().includes("agents") ? fakeEntryPoints : []),
    assistantsGet: async () => ({
      settings: structuredClone(assistants.settings),
      executable: fakeExecutable,
      profiles: structuredClone(assistants.profiles),
      snippets: [
        { client: "claude_desktop", label: "Claude Desktop", hint: "Settings › Developer › Edit Config, then restart Claude", text: config },
        { client: "claude_code", label: "Claude Code", hint: "Run in a terminal", text: `claude mcp add spiderlens -- ${fakeExecutable} mcp` },
        { client: "cursor", label: "Cursor", hint: "Settings › MCP › Add new global MCP server", text: config },
      ],
    }),
    assistantsSet: async (s) => {
      const bad = s.allowed_folders.find((f) => assistants.missing.has(f));
      if (bad) throw `${bad} is not a folder on this computer.`;
      // As the app checks it: with crawling on, every allowed profile must be saved and respect robots.txt.
      if (s.allow_crawl) {
        for (const name of s.allowed_profiles) {
          const p = assistants.profiles.find((x) => x.name === name);
          if (!p) throw `There is no saved profile named "${name}".`;
          if (!p.respects_robots) throw `The profile "${name}" ignores robots.txt, so an AI assistant cannot use it.`;
        }
      }
      const saved = { ...s, allowed_folders: [...new Set(s.allowed_folders)], allowed_profiles: [...new Set(s.allowed_profiles)] };
      assistants.settings = saved;
      assistants.saves.push(structuredClone(saved));
      return structuredClone(saved);
    },
    assistantsTest: async (): Promise<McpSelfTest> => {
      if (assistants.testError) throw assistants.testError;
      return {
        server: "spiderlens",
        version: "0.1.0",
        protocol: "2026-07-28",
        read_access: assistants.settings.allow_read,
        crawl_access: assistants.settings.allow_crawl,
        tools: [
          "list_crawls", "crawl_summary", "list_tabs", "query_pages", "page_details", "page_html", "page_agent_findings", "export_rows",
          ...(assistants.settings.allow_crawl ? ["start_crawl", "crawl_progress", "pause_crawl", "resume_crawl", "stop_crawl"] : []),
        ],
      };
    },
    assistantsActivity: async (limit) => assistants.activity.slice(0, limit),
    assistantsActivityClear: async () => {
      assistants.activity = [];
    },
    pickFolder: async () => assistants.folders.shift() ?? null,
  };
}
