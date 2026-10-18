// In-memory external data (spec 024) for UI tests. The real behaviour is
// tested in Rust against fake Google services (crates/integrations/tests).

import type { Connection } from "../generated/Connection";
import type { Ga4Request } from "../generated/Ga4Request";
import type { Ga4Stream } from "../generated/Ga4Stream";
import type { InspectPlan } from "../generated/InspectPlan";
import type { InspectRequest } from "../generated/InspectRequest";
import type { FieldRequest } from "../generated/FieldRequest";
import type { LabRequest } from "../generated/LabRequest";
import type { PerfEstimate } from "../generated/PerfEstimate";
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
  | "integrationsDefaultInspectRequest"
  | "integrationsInspectionPlan"
  | "integrationsRunInspection"
  | "integrationsAnalyticsStreams"
  | "integrationsDefaultAnalyticsRequest"
  | "integrationsRunAnalytics"
  | "onIntegrationsProgress"
  | "integrationsSaveGoogleKey"
  | "integrationsEstimateFieldData"
  | "integrationsEstimateLabTests"
  | "integrationsRunFieldData"
  | "integrationsRunLabTests"
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
  /** URL Inspection and Analytics requests the UI started, in order. */
  inspectRequests: InspectRequest[];
  analyticsRequests: Ga4Request[];
  /** What the inspection plan says; fields not set read as a 120-URL plan against a fresh allowance. */
  plan: Partial<InspectPlan>;
  /** Calls the UI made that tests check, such as "pause" or "forget google/ana@example.com". */
  calls: string[];
  /** The next run stops at the daily allowance instead of finishing. */
  quotaNext: boolean;
  /** Sends a progress event, as the app does during a run. */
  progress(p: Partial<RunProgress>): void;
  /** Lets the run that is in progress finish. */
  finishRun(): void;
  orphans: string[];
  /** Landing pages with sessions that the crawl never found. */
  analyticsOrphans: string[];
  /** Crawled pages a field data or lab test run would cover before the page cap. */
  pages: number;
  /** Field data and lab test requests the UI started, in order. */
  fieldRequests: FieldRequest[];
  labRequests: LabRequest[];
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
    inspectRequests: [],
    analyticsRequests: [],
    plan: {},
    calls: [],
    quotaNext: false,
    orphans: [],
    analyticsOrphans: [],
    pages: 300,
    fieldRequests: [],
    labRequests: [],
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
      available: ["search_console", "url_inspection", "analytics", "field_data", "lab_tests", "ahrefs", "majestic", "moz", "sheets"].includes(provider),
      settings: fi.settings[provider] ?? { enabled: false, per_minute: null, concurrency: null },
      per_minute: 600,
      concurrency: 4,
      published_per_minute: 1200,
      published_concurrency: 10,
      connections: fi.connections.filter((c) => (google ? c.kind === "google" && c.scopes.includes(scope) : c.kind === provider)).map((c) => c.label),
    };
  };
  const start = (path: string, request: { property: string } | null, resumeId: number | null, provider: Provider = "search_console") =>
    new Promise<RunProgress>((resolve) => {
      const id = resumeId ?? (fi.runs[0]?.id ?? 0) + 1;
      if (resumeId === null) fi.runs.unshift(run(id, { provider, property: request?.property ?? null }));
      fi.calls.push(`${resumeId === null ? "run" : "resume"} ${path}`);
      pending = () => {
        const r = fi.runs.find((x) => x.id === id)!;
        const quota = fi.quotaNext;
        fi.quotaNext = false;
        const finished = "2026-10-14T08:01:00Z";
        const outcome: Record<string, [Partial<RunProgress>, Partial<RunProgress>]> = {
          search_console: [
            { state: "paused_quota", done: 25000, resume_at: "2026-10-15T07:00:00Z", message: "Daily allowance used." },
            { state: "finished", done: 60010, matched: 10, unmatched: 60000, finished_at: finished },
          ],
          url_inspection: [
            { state: "paused_quota", requested: 3000, done: 2000, resume_at: "2026-10-15T07:00:00Z", message: "Today's URL Inspection allowance for this property is used up (2000 of 2000). 1000 URLs are left." },
            { state: "finished", requested: 120, done: 120, finished_at: finished },
          ],
          analytics: [
            { state: "paused_quota", requested: 480, done: 200, resume_at: "2026-10-14T09:00:00Z", message: "Analytics' hourly token allowance for this property is used up." },
            { state: "finished", requested: 480, done: 480, matched: 450, unmatched: 30, finished_at: finished },
          ],
        };
        const [paused, done] = outcome[r.provider] ?? [outcome.search_console[0], { state: "finished", done: r.requested, finished_at: finished }];
        Object.assign(r, quota ? paused : done);
        listeners.forEach((cb) => cb({ ...r }));
        resolve({ ...r });
      };
    });

  const startPerf = (provider: Provider, path: string, params: RunProgress["params"], requested: number) =>
    new Promise<RunProgress>((resolve) => {
      const id = (fi.runs[0]?.id ?? 0) + 1;
      fi.runs.unshift(run(id, { provider, account_label: "main", property: null, params, requested }));
      fi.calls.push(`run ${provider} ${path}`);
      pending = () => {
        const r = fi.runs.find((x) => x.id === id)!;
        const quota = fi.quotaNext;
        fi.quotaNext = false;
        Object.assign(r, quota
          ? { state: "paused_quota", done: Math.floor(requested / 2), resume_at: "2026-10-15T07:00:00Z", message: "The provider's daily allowance is used up." }
          : { state: "finished", done: requested, finished_at: "2026-10-14T08:01:00Z" });
        listeners.forEach((cb) => cb({ ...r }));
        resolve({ ...r });
      };
    });
  const pagesOf = (max: number | null) => Math.min(fi.pages, max ?? fi.pages);
  const estimate = (pages: number, perPage: number, perMinute: number, concurrency: number, seconds: number, allowance: number | null): PerfEstimate => {
    const requests = pages * perPage;
    return {
      urls: pages,
      requests,
      per_minute: perMinute,
      concurrency,
      duration_secs: Math.max(Math.ceil((requests * 60) / perMinute), Math.ceil((requests * seconds) / concurrency)),
      days_needed: allowance ? Math.max(1, Math.ceil(requests / allowance)) : 1,
      share_of_allowance: allowance ? requests / allowance : null,
    };
  };

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
      const before = fi.connections.find((x) => x.kind === "google" && x.label === "ana@example.com");
      const scopes = [...new Set([...(before?.scopes ?? []), ...features])];
      const c: Connection = { kind: "google", label: "ana@example.com", scopes, created_at: "2026-10-14T08:00:00Z", store: fi.store };
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
    integrationsOrphans: async (_path, source, offset, limit) => {
      const analytics = source === "analytics";
      const list = analytics ? fi.analyticsOrphans : fi.orphans;
      return {
        rows: list.slice(offset, offset + limit).map((url) => ({
          source: analytics ? "analytics" : "search_console",
          url,
          clicks: analytics ? null : 3,
          impressions: analytics ? null : 40,
          sessions: analytics ? 12 : null,
        })),
        total: list.length,
      };
    },
    integrationsRemove: async (path, provider) => {
      fi.calls.push(`remove ${provider ?? "all"} ${path}`);
      fi.runs = provider ? fi.runs.filter((r) => r.provider !== provider) : [];
    },
    integrationsCacheInfo: async () => ({ bytes: 2048, entries: 3 }),
    integrationsCacheClear: async () => 3,
    integrationsOpenTerms: async (provider) => void fi.calls.push(`terms ${provider}`),
    integrationsDefaultInspectRequest: async (property) => ({ property, urls: "indexable", max: null, fresh: false }),
    integrationsInspectionPlan: async () => ({
      selected: 120,
      allowance: 2000,
      used_today: 0,
      remaining_today: 2000,
      resets_at: "2026-10-15T07:00:00Z",
      days_needed: 1,
      ...fi.plan,
    }),
    integrationsRunInspection: (path, _account, request) => {
      fi.inspectRequests.push(request);
      return start(path, request, null, "url_inspection");
    },
    integrationsAnalyticsStreams: async (): Promise<Ga4Stream[]> => [
      { property: "properties/123", property_name: "Shop", stream: "456", stream_name: "Website", default_uri: "https://example.com", display: "Shop: Website (https://example.com)", suggested: true },
      { property: "properties/124", property_name: "Blog", stream: "999", stream_name: "Blog site", default_uri: "https://blog.example.com", display: "Blog: Blog site (https://blog.example.com)", suggested: false },
    ],
    integrationsDefaultAnalyticsRequest: async (property, stream) => ({
      property,
      stream,
      start_date: "2026-09-16",
      end_date: "2026-10-13",
      keep_query: false,
      fresh: false,
      page_size: null,
    }),
    integrationsRunAnalytics: (path, _account, request) => {
      fi.analyticsRequests.push(request);
      return start(path, { property: request.property }, null, "analytics");
    },
    onIntegrationsProgress: async (cb) => {
      listeners.add(cb);
      return () => void listeners.delete(cb);
    },
    integrationsSaveGoogleKey: async (label) => {
      const made = (["field_data", "lab_tests"] as const).map((kind): Connection => ({ kind, label, scopes: [], created_at: "2026-10-14T08:00:00Z", store: fi.store }));
      fi.calls.push(`save google key ${label}`);
      fi.connections = [...fi.connections.filter((c) => !(c.label === label && (c.kind === "field_data" || c.kind === "lab_tests"))), ...made];
      return made;
    },
    integrationsEstimateFieldData: async (_path, request) => estimate(pagesOf(request.max), request.devices.length, 140, 8, 1, null),
    integrationsEstimateLabTests: async (_path, request) =>
      estimate(pagesOf(request.max), request.strategy === "both" ? 2 : 1, 120, 6, 25, 25000),
    integrationsRunFieldData: (path, _key, request) => {
      fi.fieldRequests.push(request);
      return startPerf("field_data", path, request as unknown as RunProgress["params"], pagesOf(request.max));
    },
    integrationsRunLabTests: (path, _key, request) => {
      fi.labRequests.push(request);
      return startPerf("lab_tests", path, request as unknown as RunProgress["params"], pagesOf(request.max));
    },
  };
  return { api, integrations: fi };
}
