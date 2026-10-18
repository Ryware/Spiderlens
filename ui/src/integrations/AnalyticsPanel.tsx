// Google Analytics 4 sessions per landing page for the open crawl (spec 024
// US4): pick a property and web data stream, a date range, and whether
// landing pages that differ by query string stay apart. Landing pages the
// crawl never found are listed as orphan candidates.

import { useCallback, useEffect, useState } from "react";
import { Activity, Pause, Play, Square, Trash2 } from "lucide-react";
import { Bar, Button, Input, Notice, Select } from "../components/ui";
import type { Ga4Request } from "../generated/Ga4Request";
import type { Ga4Stream } from "../generated/Ga4Stream";
import type { OrphanPage } from "../generated/OrphanPage";
import type { ProviderStatus } from "../generated/ProviderStatus";
import type { RunProgress } from "../generated/RunProgress";
import type { Transport } from "../transport";
import { RunList, errorMessage, num, resumable } from "./runUi";

const keyOf = (s: Ga4Stream) => `${s.property}/${s.stream}`;

export function AnalyticsPanel({
  transport,
  path,
  provider,
  account,
  crawlRunning,
  onDataChanged,
  onConnected,
}: {
  transport: Transport;
  path: string;
  provider: ProviderStatus;
  /** A Google account that was allowed to read Analytics, if any. */
  account: string | null;
  crawlRunning: boolean;
  onDataChanged: () => void;
  /** A sign-in finished and the connections changed. */
  onConnected: () => void;
}) {
  const [streams, setStreams] = useState<Ga4Stream[] | null>(null);
  const [request, setRequest] = useState<Ga4Request | null>(null);
  const [runs, setRuns] = useState<RunProgress[]>([]);
  const [live, setLive] = useState<RunProgress | null>(null);
  const [busy, setBusy] = useState(false);
  const [waiting, setWaiting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [orphans, setOrphans] = useState<OrphanPage | null>(null);
  const ready = provider.settings.enabled && account !== null;

  const loadRuns = useCallback(() => {
    transport.integrationsRuns(path).then((all) => setRuns(all.filter((r) => r.provider === "analytics")), () => {});
    transport.integrationsOrphans(path, "analytics", 0, 50).then(setOrphans, () => {});
  }, [transport, path]);
  useEffect(loadRuns, [loadRuns]);

  useEffect(() => {
    if (!ready) return;
    let off = false;
    transport.integrationsAnalyticsStreams(account, path).then(
      async (list) => {
        if (off) return;
        setStreams(list);
        const pick = list.find((s) => s.suggested) ?? list[0];
        if (pick) {
          const r = await transport.integrationsDefaultAnalyticsRequest(pick.property, pick.stream);
          if (!off) setRequest(r);
        }
      },
      (e) => !off && setError(errorMessage(e)),
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

  const pickStream = (key: string) => {
    const s = streams?.find((x) => keyOf(x) === key);
    if (s && request) setRequest({ ...request, property: s.property, stream: s.stream });
  };
  const allow = async () => {
    setError(null);
    setWaiting(true);
    try {
      await transport.integrationsConnectGoogle(["analytics"]);
      onConnected();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setWaiting(false);
    }
  };
  const go = async (start: () => Promise<RunProgress>) => {
    setError(null);
    setBusy(true);
    try {
      setLive(await start());
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
      loadRuns();
      onDataChanged();
    }
  };
  const remove = async () => {
    try {
      await transport.integrationsRemove(path, "analytics");
      setLive(null);
      loadRuns();
      onDataChanged();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const paused = runs.find((r) => resumable(r.state) && r.state !== "running");
  const shown = busy && live?.provider === "analytics" ? live : null;

  return (
    <section aria-labelledby="ga-run-heading" className="flex flex-col gap-3 rounded-lg border border-line p-4">
      <div className="flex items-center gap-2">
        <Activity aria-hidden className="size-4 text-fg-3" />
        <h2 id="ga-run-heading" className="m-0 text-base font-semibold text-fg">Analytics for this crawl</h2>
      </div>
      {!provider.settings.enabled ? (
        <Notice>Turn on Google Analytics 4 above to fetch sessions, engagement, key events and revenue for each landing page.</Notice>
      ) : account === null ? (
        <div className="flex flex-col items-start gap-2">
          <Notice>Analytics needs read-only access to your Google Analytics data. Your Search Console sign-in does not include it.</Notice>
          <div className="flex items-center gap-2">
            <Button disabled={waiting} onClick={() => void allow()}>Allow Google Analytics access</Button>
            {waiting && (
              <>
                <span role="status" className="text-sm text-fg-2">Finish signing in in your browser…</span>
                <Button size="sm" variant="quiet" onClick={() => void transport.integrationsConnectCancel()}>Cancel</Button>
              </>
            )}
          </div>
        </div>
      ) : (
        <>
          <div className="flex flex-wrap items-end gap-3">
            <label className="flex min-w-60 flex-1 flex-col gap-1 text-sm text-fg-2">
              Data stream
              <Select
                value={request ? `${request.property}/${request.stream}` : ""}
                disabled={!streams || busy}
                onChange={(e) => pickStream(e.target.value)}
              >
                {streams?.length === 0 && <option value="">No web data streams for this account</option>}
                {streams?.map((s) => (
                  <option key={keyOf(s)} value={keyOf(s)}>
                    {s.display}
                    {s.suggested ? " (matches this crawl)" : ""}
                  </option>
                ))}
              </Select>
            </label>
            <label className="flex flex-col gap-1 text-sm text-fg-2">
              Analytics from
              <Input type="date" value={request?.start_date ?? ""} disabled={busy} onChange={(e) => request && setRequest({ ...request, start_date: e.target.value })} />
            </label>
            <label className="flex flex-col gap-1 text-sm text-fg-2">
              Analytics to
              <Input type="date" value={request?.end_date ?? ""} disabled={busy} onChange={(e) => request && setRequest({ ...request, end_date: e.target.value })} />
            </label>
          </div>
          <label className="flex items-center gap-2 text-sm text-fg-2">
            <input
              type="checkbox"
              className="size-3.5 accent-accent"
              checked={request?.keep_query ?? false}
              disabled={!request || busy}
              onChange={(e) => request && setRequest({ ...request, keep_query: e.target.checked })}
            />
            Keep landing pages that differ by query string apart
          </label>
          <p className="m-0 text-xs text-fg-3">{provider.sends} Analytics allows a limited number of tokens an hour and a day; a run stops before they run out and continues later.</p>
          {crawlRunning && <Notice kind="warn">This crawl is still running. Fetch external data when it has finished or paused.</Notice>}
          <div className="flex flex-wrap items-center gap-2">
            {!busy && (
              <Button
                variant="primary"
                disabled={!request || crawlRunning}
                onClick={() => request && void go(() => transport.integrationsRunAnalytics(path, account, request))}
              >
                <Play aria-hidden className="mr-1 size-3.5" />
                Fetch Analytics data
              </Button>
            )}
            {!busy && paused && (
              <Button disabled={crawlRunning} onClick={() => void go(() => transport.integrationsRunResume(path, paused.id))}>
                Resume Analytics run {paused.id}
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
        <div role="status" aria-label="Analytics progress" className="flex items-center gap-3 text-sm text-fg-2">
          <Bar value={shown.requested ? shown.done / shown.requested : 0.5} tone="accent" className="w-32" />
          <span>{num(shown.done)} landing pages read</span>
        </div>
      )}
      {error && <Notice kind="bad" role="alert">{error}</Notice>}

      <RunList
        label="Analytics runs"
        runs={runs}
        summary={(r) => (
          <>
            {num(r.done)} landing pages
            {r.matched !== null && `, ${num(r.matched)} matched crawled pages`}
            {r.unmatched ? `, ${num(r.unmatched)} not in the crawl` : ""}
          </>
        )}
      />

      {orphans && orphans.total > 0 && (
        <details>
          <summary className="cursor-pointer text-sm text-fg">
            {num(orphans.total)} landing pages with sessions were not found by the crawl
          </summary>
          <ul aria-label="Analytics orphan candidates" className="m-0 mt-2 flex max-h-64 list-none flex-col gap-0.5 overflow-y-auto p-0 font-code text-xs">
            {orphans.rows.map((o) => (
              <li key={o.url} className="flex gap-2">
                <span className="min-w-0 flex-1 truncate">{o.url}</span>
                <span className="text-fg-3">{num(o.sessions)} sessions</span>
              </li>
            ))}
          </ul>
        </details>
      )}

      {runs.length > 0 && !busy && (
        <div>
          <Button size="sm" variant="destructive" onClick={() => void remove()}>
            <Trash2 aria-hidden className="mr-1 size-3.5" />
            Remove Analytics data from this crawl
          </Button>
        </div>
      )}
    </section>
  );
}
