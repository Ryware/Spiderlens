// In-memory AI agent findings and assistant settings for UI tests (spec 016).
// The real collection, storage and MCP server are tested in Rust
// (crates/engine/tests/agents.rs, crates/results/tests/agents.rs,
// crates/mcp/tests/server.rs).

import type { ActivityEntry } from "../generated/ActivityEntry";
import type { AssistantSettings } from "../generated/AssistantSettings";
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

export function fakeAgentsTransport(
  urls: () => FakeUrl[],
  features: () => Need[],
): Pick<
  Transport,
  "agentsPage" | "assistantsGet" | "assistantsSet" | "assistantsTest" | "assistantsActivity" | "assistantsActivityClear" | "pickFolder"
> & { assistants: FakeAssistants } {
  const assistants: FakeAssistants = {
    settings: { allow_read: false, allowed_folders: [], allow_crawl: false, allowed_profiles: [] },
    activity: [],
    folders: ["/Users/me/Crawls"],
    missing: new Set(),
    testError: null,
    saves: [],
  };
  const config = JSON.stringify({ mcpServers: { spiderlens: { command: fakeExecutable, args: ["mcp"] } } }, null, 2);
  return {
    assistants,
    agentsPage: async (_p, id) => fakeFindings(urls().find((u) => u.id === id)!, features()),
    assistantsGet: async () => ({
      settings: structuredClone(assistants.settings),
      executable: fakeExecutable,
      snippets: [
        { client: "claude_desktop", label: "Claude Desktop", hint: "Settings › Developer › Edit Config, then restart Claude", text: config },
        { client: "claude_code", label: "Claude Code", hint: "Run in a terminal", text: `claude mcp add spiderlens -- ${fakeExecutable} mcp` },
        { client: "cursor", label: "Cursor", hint: "Settings › MCP › Add new global MCP server", text: config },
      ],
    }),
    assistantsSet: async (s) => {
      const bad = s.allowed_folders.find((f) => assistants.missing.has(f));
      if (bad) throw `${bad} is not a folder on this computer.`;
      const saved = { ...s, allowed_folders: [...new Set(s.allowed_folders)], allow_crawl: false, allowed_profiles: [] };
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
        crawl_access: false,
        tools: ["list_crawls", "crawl_summary", "list_tabs", "query_pages", "page_details", "page_html", "page_agent_findings", "export_rows"],
      };
    },
    assistantsActivity: async (limit) => assistants.activity.slice(0, limit),
    assistantsActivityClear: async () => {
      assistants.activity = [];
    },
    pickFolder: async () => assistants.folders.shift() ?? null,
  };
}
