// External data (spec 024): connections, which providers are on and what
// each sends, and the run panel that fetches data into the open crawl.
// Everything runs after a crawl, from stored URLs; nothing here touches the
// crawl itself.

import { useCallback, useEffect, useState } from "react";
import { Cable, KeyRound, Pause, Play, Search, Square, Trash2 } from "lucide-react";
import { Badge, Bar, Button, Input, Notice, Select, Switch, cn } from "../components/ui";
import type { OrphanPage } from "../generated/OrphanPage";
import type { PageRequest } from "../generated/PageRequest";
import type { Property } from "../generated/Property";
import type { ProviderStatus } from "../generated/ProviderStatus";
import type { RunProgress } from "../generated/RunProgress";
import type { RunState } from "../generated/RunState";
import type { Status } from "../generated/Status";
import type { Transport } from "../transport";
import { AnalyticsPanel } from "./AnalyticsPanel";
import { InspectionPanel } from "./InspectionPanel";

const STATE_LABEL: Record<RunState, string> = {
  queued: "Queued",
  running: "Running",
  finished: "Finished",
  paused_user: "Paused",
  paused_quota: "Paused: daily allowance used",
  cancelled: "Cancelled",
  failed: "Failed",
};
const STATE_TONE: Record<RunState, "ok" | "info" | "warn" | "bad" | "neutral"> = {
  queued: "neutral",
  running: "info",
  finished: "ok",
  paused_user: "warn",
  paused_quota: "warn",
  cancelled: "neutral",
  failed: "bad",
};
const resumable = (s: RunState) => s === "paused_user" || s === "paused_quota" || s === "running" || s === "queued";
const n = (v: number | null) => (v ?? 0).toLocaleString();

function message(e: unknown) {
  return e instanceof Error ? e.message : String(e);
}

/** The Integrations screen. `path` is the open crawl file, if any. */
export function IntegrationsView({ transport, path, crawlRunning, onDataChanged }: { transport: Transport; path: string | null; crawlRunning: boolean; onDataChanged: () => void }) {
  const [status, setStatus] = useState<Status | null>(null);
  const [error, setError] = useState<string | null>(null);
  const reload = useCallback(() => transport.integrationsStatus().then(setStatus, (e) => setError(message(e))), [transport]);
  useEffect(() => void reload(), [reload]);

  if (!status) return <div className="p-6 text-sm text-fg-3">{error ?? "Loading…"}</div>;
  const sc = status.providers.find((p) => p.provider === "search_console")!;
  const inspection = status.providers.find((p) => p.provider === "url_inspection")!;
  const analytics = status.providers.find((p) => p.provider === "analytics")!;
  const google = status.connections.filter((c) => c.kind === "google");

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto flex max-w-3xl flex-col gap-6 p-6">
        <header className="flex flex-col gap-1">
          <h1 className="m-0 text-lg font-semibold text-fg">Integrations</h1>
          <p className="m-0 text-sm text-fg-2">
            Add data from Google and SEO tools to a finished crawl. Every source is off until you turn it on, runs only when you start it, and uses your own account or key.
          </p>
          {status.store === "memory_only" && (
            <Notice kind="warn" role="status">
              No keychain is available on this computer. Sign-ins and keys are kept only until SpiderLens quits.
            </Notice>
          )}
          {error && <Notice kind="bad" role="alert">{error}</Notice>}
        </header>

        <GoogleSection transport={transport} status={status} onChanged={reload} onError={setError} />

        <section aria-labelledby="sources-heading" className="flex flex-col gap-2">
          <h2 id="sources-heading" className="m-0 text-base font-semibold text-fg">Data sources</h2>
          <ul aria-label="Data sources" className="m-0 flex list-none flex-col divide-y divide-line rounded-lg border border-line p-0">
            {status.providers.map((p) => (
              <ProviderRow key={p.provider} transport={transport} p={p} onChanged={reload} onError={setError} />
            ))}
          </ul>
        </section>

        {path ? (
          <SearchConsoleRun
            transport={transport}
            path={path}
            provider={sc}
            account={google.find((c) => c.scopes.includes("search_console"))?.label ?? null}
            crawlRunning={crawlRunning}
            onDataChanged={onDataChanged}
          />
        ) : (
          <Notice>Open or finish a crawl to fetch Search Console data for it.</Notice>
        )}
        {path && (
          <>
            <InspectionPanel
              transport={transport}
              path={path}
              provider={inspection}
              account={google.find((c) => c.scopes.includes("search_console"))?.label ?? null}
              crawlRunning={crawlRunning}
              onDataChanged={onDataChanged}
            />
            <AnalyticsPanel
              transport={transport}
              path={path}
              provider={analytics}
              account={google.find((c) => c.scopes.includes("analytics"))?.label ?? null}
              crawlRunning={crawlRunning}
              onDataChanged={onDataChanged}
              onConnected={() => void reload()}
            />
          </>
        )}
      </div>
    </div>
  );
}

function GoogleSection({ transport, status, onChanged, onError }: { transport: Transport; status: Status; onChanged: () => void; onError: (e: string | null) => void }) {
  const [waiting, setWaiting] = useState(false);
  const [ownOpen, setOwnOpen] = useState(false);
  const [clientId, setClientId] = useState("");
  const [secret, setSecret] = useState("");
  const google = status.connections.filter((c) => c.kind === "google");

  const connect = async () => {
    onError(null);
    setWaiting(true);
    try {
      await transport.integrationsConnectGoogle(["search_console"]);
      onChanged();
    } catch (e) {
      onError(message(e));
    } finally {
      setWaiting(false);
    }
  };
  const forget = async (label: string) => {
    try {
      const revoked = await transport.integrationsForget("google", label);
      onError(revoked ? null : "Removed here. Google did not confirm; you can also remove access in your Google account's security settings.");
      onChanged();
    } catch (e) {
      onError(message(e));
    }
  };
  const saveClient = async (id: string) => {
    try {
      await transport.integrationsGoogleClient(id, secret || null);
      setOwnOpen(false);
      setSecret("");
      onChanged();
    } catch (e) {
      onError(message(e));
    }
  };

  return (
    <section aria-labelledby="google-heading" className="flex flex-col gap-2">
      <h2 id="google-heading" className="m-0 text-base font-semibold text-fg">Google account</h2>
      {google.length > 0 && (
        <ul aria-label="Google accounts" className="m-0 flex list-none flex-col gap-1 p-0">
          {google.map((c) => (
            <li key={c.label} className="flex items-center gap-2 rounded-md bg-surface-2 px-3 py-2 text-sm">
              <Cable aria-hidden className="size-4 text-fg-3" />
              <span className="min-w-0 flex-1 truncate text-fg">{c.label}</span>
              <span className="text-xs text-fg-3">{c.scopes.map((s) => s.replace("_", " ")).join(", ")}</span>
              <Button size="sm" variant="destructive" onClick={() => void forget(c.label)} aria-label={`Forget ${c.label}`}>
                Forget
              </Button>
            </li>
          ))}
        </ul>
      )}
      {status.google_client ? (
        <div className="flex items-center gap-2">
          <Button variant={google.length ? "secondary" : "primary"} disabled={waiting} onClick={() => void connect()}>
            {google.length ? "Connect another account" : "Connect Google account"}
          </Button>
          {waiting && (
            <>
              <span role="status" className="text-sm text-fg-2">Finish signing in in your browser…</span>
              <Button size="sm" variant="quiet" onClick={() => void transport.integrationsConnectCancel()}>Cancel</Button>
            </>
          )}
        </div>
      ) : (
        <Notice>This build has no Google client. Add your own OAuth client (type Desktop) from Google Cloud to connect.</Notice>
      )}
      <p className="m-0 text-xs text-fg-3">
        Read-only access. The sign-in is kept in your system keychain, never in crawl files or exports.
        {status.own_google_client && " Using your own Google client."}
      </p>
      <div>
        <Button size="sm" variant="quiet" aria-expanded={ownOpen} onClick={() => setOwnOpen((o) => !o)}>
          <KeyRound aria-hidden className="mr-1 size-3.5" />
          {status.own_google_client ? "Change your Google client" : "Use your own Google client"}
        </Button>
      </div>
      {ownOpen && (
        <form
          aria-label="Your Google client"
          className="flex flex-col gap-2 rounded-md border border-line p-3"
          onSubmit={(e) => {
            e.preventDefault();
            void saveClient(clientId);
          }}
        >
          <label className="flex flex-col gap-1 text-sm text-fg-2">
            Client ID
            <Input mono value={clientId} onChange={(e) => setClientId(e.target.value)} placeholder="….apps.googleusercontent.com" />
          </label>
          <label className="flex flex-col gap-1 text-sm text-fg-2">
            Client secret (optional for Desktop clients)
            <Input type="password" mono value={secret} onChange={(e) => setSecret(e.target.value)} autoComplete="off" />
          </label>
          <div className="flex gap-2">
            <Button type="submit" variant="primary" size="sm" disabled={!clientId.trim()}>Save to keychain</Button>
            {status.own_google_client && (
              <Button size="sm" variant="quiet" onClick={() => void saveClient("")}>Use the built-in client</Button>
            )}
          </div>
        </form>
      )}
    </section>
  );
}

function ProviderRow({ transport, p, onChanged, onError }: { transport: Transport; p: ProviderStatus; onChanged: () => void; onError: (e: string | null) => void }) {
  const id = `provider-${p.provider}`;
  const toggle = async (enabled: boolean) => {
    try {
      await transport.integrationsSettingsSave(p.provider, { ...p.settings, enabled });
      onChanged();
    } catch (e) {
      onError(message(e));
    }
  };
  return (
    <li data-provider={p.provider} className="flex items-start gap-3 px-3 py-2.5">
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span id={id} className={cn("text-sm font-medium", p.available ? "text-fg" : "text-fg-3")}>{p.label}</span>
          {!p.available && <Badge>Coming later</Badge>}
          {p.available && p.settings.enabled && p.connections.length === 0 && <Badge tone="warn">Not connected</Badge>}
        </div>
        <p className="m-0 text-xs text-fg-3">{p.sends}</p>
        {p.available && (
          <button type="button" className="text-xs text-accent-text underline-offset-2 hover:underline" onClick={() => void transport.integrationsOpenTerms(p.provider)}>
            Terms of use
          </button>
        )}
      </div>
      <Switch checked={p.settings.enabled} disabled={!p.available} labelledBy={id} onChange={(v) => void toggle(v)} />
    </li>
  );
}

function SearchConsoleRun({
  transport,
  path,
  provider,
  account,
  crawlRunning,
  onDataChanged,
}: {
  transport: Transport;
  path: string;
  provider: ProviderStatus;
  account: string | null;
  crawlRunning: boolean;
  onDataChanged: () => void;
}) {
  const [properties, setProperties] = useState<Property[] | null>(null);
  const [request, setRequest] = useState<PageRequest | null>(null);
  const [runs, setRuns] = useState<RunProgress[]>([]);
  const [live, setLive] = useState<RunProgress | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [orphans, setOrphans] = useState<OrphanPage | null>(null);
  const ready = provider.settings.enabled && account !== null;

  const loadRuns = useCallback(() => {
    transport.integrationsRuns(path).then(setRuns, () => {});
    transport.integrationsOrphans(path, "search_console", 0, 50).then(setOrphans, () => {});
  }, [transport, path]);
  useEffect(loadRuns, [loadRuns]);

  useEffect(() => {
    if (!ready) return;
    let off = false;
    transport.integrationsProperties(account, path).then(
      async (list) => {
        if (off) return;
        setProperties(list);
        const pick = list.find((p) => p.suggested) ?? list[0];
        if (pick) {
          const r = await transport.integrationsDefaultRequest(pick.id);
          if (!off) setRequest(r);
        }
      },
      (e) => !off && setError(message(e)),
    );
    return () => {
      off = true;
    };
  }, [transport, account, path, ready]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let off = false;
    transport.onIntegrationsProgress((p) => setLive(p)).then((u) => (off ? u() : (unlisten = u)));
    return () => {
      off = true;
      unlisten?.();
    };
  }, [transport]);

  const go = async (start: () => Promise<RunProgress>) => {
    setError(null);
    setBusy(true);
    try {
      setLive(await start());
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
      loadRuns();
      onDataChanged();
    }
  };
  const remove = async () => {
    try {
      await transport.integrationsRemove(path, "search_console");
      setLive(null);
      loadRuns();
      onDataChanged();
    } catch (e) {
      setError(message(e));
    }
  };

  const paused = runs.find((r) => r.provider === "search_console" && resumable(r.state) && r.state !== "running");
  const shown = busy ? live : null;

  return (
    <section aria-labelledby="sc-run-heading" className="flex flex-col gap-3 rounded-lg border border-line p-4">
      <div className="flex items-center gap-2">
        <Search aria-hidden className="size-4 text-fg-3" />
        <h2 id="sc-run-heading" className="m-0 text-base font-semibold text-fg">Search Console for this crawl</h2>
      </div>
      {!provider.settings.enabled ? (
        <Notice>Turn on Search Console above to fetch clicks, impressions, CTR and position for the crawled pages.</Notice>
      ) : account === null ? (
        <Notice>Connect a Google account that can read Search Console.</Notice>
      ) : (
        <>
          <div className="flex flex-wrap items-end gap-3">
            <label className="flex min-w-60 flex-1 flex-col gap-1 text-sm text-fg-2">
              Property
              <Select
                value={request?.property ?? ""}
                disabled={!properties || busy}
                onChange={(e) => request && setRequest({ ...request, property: e.target.value })}
              >
                {properties?.length === 0 && <option value="">No properties for this account</option>}
                {properties?.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.display}
                    {p.suggested ? " (matches this crawl)" : ""}
                  </option>
                ))}
              </Select>
            </label>
            <label className="flex flex-col gap-1 text-sm text-fg-2">
              From
              <Input type="date" value={request?.start_date ?? ""} disabled={busy} onChange={(e) => request && setRequest({ ...request, start_date: e.target.value })} />
            </label>
            <label className="flex flex-col gap-1 text-sm text-fg-2">
              To
              <Input type="date" value={request?.end_date ?? ""} disabled={busy} onChange={(e) => request && setRequest({ ...request, end_date: e.target.value })} />
            </label>
          </div>
          <p className="m-0 text-xs text-fg-3">{provider.sends} Search Console data trails by about three days.</p>
          {crawlRunning && <Notice kind="warn">This crawl is still running. Fetch external data when it has finished or paused.</Notice>}
          <div className="flex flex-wrap items-center gap-2">
            {!busy && (
              <Button variant="primary" disabled={!request || crawlRunning} onClick={() => request && void go(() => transport.integrationsRunSearchConsole(path, account, request))}>
                <Play aria-hidden className="mr-1 size-3.5" />
                Fetch Search Console data
              </Button>
            )}
            {!busy && paused && (
              <Button disabled={crawlRunning} onClick={() => void go(() => transport.integrationsRunResume(path, paused.id))}>
                Resume run {paused.id}
              </Button>
            )}
            {busy && (
              <>
                <Button onClick={() => void transport.integrationsRunPause()}>
                  <Pause aria-hidden className="mr-1 size-3.5" />
                  Pause
                </Button>
                <Button variant="quiet" onClick={() => void transport.integrationsRunCancel()}>
                  <Square aria-hidden className="mr-1 size-3.5" />
                  Cancel
                </Button>
              </>
            )}
          </div>
        </>
      )}

      {shown && (
        <div role="status" aria-label="Run progress" className="flex items-center gap-3 text-sm text-fg-2">
          <Bar value={shown.requested ? shown.done / shown.requested : 0.5} tone="accent" className="w-32" />
          <span>{n(shown.done)} rows read</span>
        </div>
      )}
      {error && <Notice kind="bad" role="alert">{error}</Notice>}

      {runs.some((r) => r.provider === "search_console") && (
        <ul aria-label="Runs" className="m-0 flex list-none flex-col gap-1 p-0">
          {runs.filter((r) => r.provider === "search_console").map((r) => (
            <li key={r.id} data-run={r.id} className="flex flex-wrap items-center gap-2 text-sm text-fg-2">
              <Badge tone={STATE_TONE[r.state]}>{STATE_LABEL[r.state]}</Badge>
              <span className="text-fg">{r.property}</span>
              <span>
                {n(r.done)} rows
                {r.matched !== null && `, ${n(r.matched)} matched crawled pages`}
                {r.unmatched ? `, ${n(r.unmatched)} not in the crawl` : ""}
              </span>
              {r.state === "paused_quota" && r.resume_at && <span className="text-warn">Resume after {new Date(r.resume_at).toLocaleString()}</span>}
              {r.message && r.state !== "finished" && <span className="text-fg-3">{r.message}</span>}
            </li>
          ))}
        </ul>
      )}

      {orphans && orphans.total > 0 && (
        <details>
          <summary className="cursor-pointer text-sm text-fg">
            {n(orphans.total)} pages with search traffic were not found by the crawl
          </summary>
          <ul aria-label="Orphan candidates" className="m-0 mt-2 flex max-h-64 list-none flex-col gap-0.5 overflow-y-auto p-0 font-code text-xs">
            {orphans.rows.map((o) => (
              <li key={o.url} className="flex gap-2">
                <span className="min-w-0 flex-1 truncate">{o.url}</span>
                <span className="text-fg-3">{n(o.clicks)} clicks</span>
              </li>
            ))}
          </ul>
        </details>
      )}

      {runs.some((r) => r.provider === "search_console") && !busy && (
        <div>
          <Button size="sm" variant="destructive" onClick={() => void remove()}>
            <Trash2 aria-hidden className="mr-1 size-3.5" />
            Remove Search Console data from this crawl
          </Button>
        </div>
      )}
    </section>
  );
}
