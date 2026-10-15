// In-memory external data (spec 024) for UI tests. The real behaviour is
// tested in Rust against fake Google services (crates/integrations/tests).

import type { Connection } from "../generated/Connection";
import type { PageRequest } from "../generated/PageRequest";
import type { Provider } from "../generated/Provider";
import type { ProviderSettings } from "../generated/ProviderSettings";
import type { ProviderStatus } from "../generated/ProviderStatus";
import type { RunProgress } from "../generated/RunProgress";
import type { StoreKind } from "../generated/StoreKind";
import type { Transport } from "../transport";

const PROVIDERS: [Provider, string, boolean, string][] = [
  ["search_console", "Search Console", true, "Sends the property and date range to Google. The list of crawled URLs is not sent."],
  ["url_inspection", "URL Inspection", true, "Sends each selected URL and its property to Google."],
  ["analytics", "Google Analytics 4", true, "Sends the property, data stream and date range to Google."],
  ["field_data", "Chrome UX Report (field data)", false, "Sends each crawled page URL to Google's Chrome UX Report."],
  ["lab_tests", "PageSpeed Insights (lab tests)", false, "Sends each selected URL to Google PageSpeed Insights."],
  ["ahrefs", "Ahrefs", false, "Sends each selected URL to Ahrefs and uses your plan's API units."],
  ["majestic", "Majestic", false, "Sends each selected URL to Majestic and uses your plan's units."],
  ["moz", "Moz", false, "Sends each selected URL to Moz and uses your plan's rows."],
  ["sheets", "Google Sheets", true, "Creates a new spreadsheet in your Google Drive with the exported rows."],
];

type Integrations = Pick<
  Transport,
  | "integrationsStatus"
  | "integrationsSettingsSave"
  | "integrationsGoogleClient"
  | "integrationsConnectGoogle"
  | "integrationsConnectCancel"
  | "integrationsSaveKey"
  | "integrationsForget"
  | "integrationsProperties"
  | "integrationsDefaultRequest"
  | "integrationsRunSearchConsole"
  | "integrationsRunResume"
  | "integrationsRunPause"
  | "integrationsRunCancel"
  | "integrationsRuns"
  | "integrationsQueries"
  | "integrationsOrphans"
  | "integrationsRemove"
  | "integrationsCacheInfo"
  | "integrationsCacheClear"
  | "integrationsOpenTerms"
  | "onIntegrationsProgress"
>;

export interface FakeIntegrations {
  store: StoreKind;
  googleClient: boolean;
  ownClient: string | null;
  connections: Connection[];
  settings: Partial<Record<Provider, ProviderSettings>>;
  runs: RunProgress[];
  /** Requests the UI started, in order. */
  requests: PageRequest[];
  /** Calls the UI made that tests check, such as "pause" or "forget google/ana@example.com". */
  calls: string[];
  /** The next run stops at the daily allowance instead of finishing. */
  quotaNext: boolean;
  /** Sends a progress event, as the app does during a run. */
  progress(p: Partial<RunProgress>): void;
  /** Lets the run that is in progress finish. */
  finishRun(): void;
  orphans: string[];
}

const run = (id: number, patch: Partial<RunProgress> = {}): RunProgress => ({
  id,
  provider: "search_console",
  account_label: "ana@example.com",
  property: "sc-domain:example.com",
  params: {},
  state: "running",
  requested: 0,
  done: 0,
  failed: 0,
  matched: null,
  unmatched: null,
  usage: null,
  resume_at: null,
  message: null,
  created_at: "2026-10-14T08:00:00Z",
  finished_at: null,
  ...patch,
});

export function fakeIntegrationsTransport(): { api: Integrations; integrations: FakeIntegrations } {
  const listeners = new Set<(p: RunProgress) => void>();
  let pending: (() => void) | null = null;
  const fi: FakeIntegrations = {
    store: "keychain",
    googleClient: true,
    ownClient: null,
    connections: [],
    settings: {},
    runs: [],
    requests: [],
    calls: [],
    quotaNext: false,
    orphans: [],
    progress: (p) => listeners.forEach((cb) => cb(run(fi.runs[0]?.id ?? 1, p))),
    finishRun: () => {
      const f = pending;
      pending = null;
      f?.();
    },
  };
  const status = (p: (typeof PROVIDERS)[number]): ProviderStatus => {
    const [provider, label, google, sends] = p;
    const scope = provider === "url_inspection" ? "search_console" : provider;
    return {
      provider,
      label,
      sends,
      terms_url: "https://example.com/terms",
      uses_google_account: google,
      available: provider === "search_console",
      settings: fi.settings[provider] ?? { enabled: false, per_minute: null, concurrency: null },
      per_minute: 600,
      concurrency: 4,
      published_per_minute: 1200,
      published_concurrency: 10,
      connections: fi.connections.filter((c) => (google ? c.kind === "google" && c.scopes.includes(scope) : c.kind === provider)).map((c) => c.label),
    };
  };
  const start = (path: string, request: PageRequest | null, resumeId: number | null) =>
    new Promise<RunProgress>((resolve) => {
      const id = resumeId ?? (fi.runs[0]?.id ?? 0) + 1;
      if (resumeId === null) fi.runs.unshift(run(id, { property: request?.property ?? null }));
      fi.calls.push(`${resumeId === null ? "run" : "resume"} ${path}`);
      pending = () => {
        const r = fi.runs.find((x) => x.id === id)!;
        const quota = fi.quotaNext;
        fi.quotaNext = false;
        Object.assign(r, quota
          ? { state: "paused_quota", done: 25000, resume_at: "2026-10-15T07:00:00Z", message: "Daily allowance used." }
          : { state: "finished", done: 60010, matched: 10, unmatched: 60000, finished_at: "2026-10-14T08:01:00Z" });
        listeners.forEach((cb) => cb({ ...r }));
        resolve({ ...r });
      };
    });

  const api: Integrations = {
    integrationsStatus: async () => ({
      store: fi.store,
      google_client: fi.googleClient || fi.ownClient !== null,
      own_google_client: fi.ownClient !== null,
      connections: structuredClone(fi.connections),
      providers: PROVIDERS.map(status),
    }),
    integrationsSettingsSave: async (provider, settings) => {
      fi.settings[provider as Provider] = settings;
      return status(PROVIDERS.find((p) => p[0] === provider)!);
    },
    integrationsGoogleClient: async (clientId) => {
      fi.ownClient = clientId.trim() || null;
    },
    integrationsConnectGoogle: async (features) => {
      fi.calls.push(`connect ${features.join(",")}`);
      const c: Connection = { kind: "google", label: "ana@example.com", scopes: features, created_at: "2026-10-14T08:00:00Z", store: fi.store };
      fi.connections = [...fi.connections.filter((x) => x.label !== c.label), c];
      return c;
    },
    integrationsConnectCancel: async () => void fi.calls.push("cancel sign-in"),
    integrationsSaveKey: async (provider, label) => {
      const c: Connection = { kind: provider, label, scopes: [], created_at: "2026-10-14T08:00:00Z", store: fi.store };
      fi.connections.push(c);
      return c;
    },
    integrationsForget: async (kind, label) => {
      fi.calls.push(`forget ${kind}/${label}`);
      fi.connections = fi.connections.filter((c) => !(c.kind === kind && c.label === label));
      return kind === "google";
    },
    integrationsProperties: async () => [
      { id: "sc-domain:example.com", display: "example.com (domain)", kind: "domain", suggested: true },
      { id: "https://blog.example.com/", display: "https://blog.example.com/", kind: "url_prefix", suggested: false },
    ],
    integrationsDefaultRequest: async (property) => ({ property, start_date: "2026-09-14", end_date: "2026-10-11", search_type: "web", fresh: false }),
    integrationsRunSearchConsole: (path, _account, request) => {
      fi.requests.push(request);
      return start(path, request, null);
    },
    integrationsRunResume: (path, runId) => start(path, null, runId ?? fi.runs[0]?.id ?? 1),
    integrationsRunPause: async () => void fi.calls.push("pause"),
    integrationsRunCancel: async () => void fi.calls.push("cancel"),
    integrationsRuns: async () => structuredClone(fi.runs),
    integrationsQueries: async () => [{ query: "red shoes", clicks: 12, impressions: 340, ctr: 0.035, position: 4.2 }],
    integrationsOrphans: async (_path, _source, offset, limit) => ({
      rows: fi.orphans.slice(offset, offset + limit).map((url) => ({ source: "search_console", url, clicks: 3, impressions: 40, sessions: null })),
      total: fi.orphans.length,
    }),
    integrationsRemove: async (path, provider) => {
      fi.calls.push(`remove ${provider ?? "all"} ${path}`);
      fi.runs = provider ? fi.runs.filter((r) => r.provider !== provider) : [];
    },
    integrationsCacheInfo: async () => ({ bytes: 2048, entries: 3 }),
    integrationsCacheClear: async () => 3,
    integrationsOpenTerms: async (provider) => void fi.calls.push(`terms ${provider}`),
    onIntegrationsProgress: async (cb) => {
      listeners.add(cb);
      return () => void listeners.delete(cb);
    },
  };
  return { api, integrations: fi };
}
