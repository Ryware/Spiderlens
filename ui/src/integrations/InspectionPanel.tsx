// URL Inspection for the open crawl (spec 024 US2): choose the pages, see
// the count against today's allowance before starting, and watch the run
// pause when Google's daily allowance is used up and continue the next day.

import { useCallback, useEffect, useState } from "react";
import { Pause, Play, Square, TextSearch, Trash2 } from "lucide-react";
import { Bar, Button, Input, Notice, Select } from "../components/ui";
import type { InspectPlan } from "../generated/InspectPlan";
import type { InspectRequest } from "../generated/InspectRequest";
import type { Property } from "../generated/Property";
import type { ProviderStatus } from "../generated/ProviderStatus";
import type { RunProgress } from "../generated/RunProgress";
import type { Transport } from "../transport";
import { RunList, errorMessage, num, resumable } from "./runUi";

export function InspectionPanel({
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
  const [request, setRequest] = useState<InspectRequest | null>(null);
  const [plan, setPlan] = useState<InspectPlan | null>(null);
  const [runs, setRuns] = useState<RunProgress[]>([]);
  const [live, setLive] = useState<RunProgress | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const ready = provider.settings.enabled && account !== null;

  const loadRuns = useCallback(() => {
    transport.integrationsRuns(path).then((all) => setRuns(all.filter((r) => r.provider === "url_inspection")), () => {});
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
          const r = await transport.integrationsDefaultInspectRequest(pick.id);
          if (!off) setRequest(r);
        }
      },
      (e) => !off && setError(errorMessage(e)),
    );
    return () => {
      off = true;
    };
  }, [transport, account, path, ready]);

  const refreshPlan = useCallback(() => {
    if (!ready || !request) return;
    transport.integrationsInspectionPlan(path, account, request).then(setPlan, (e) => setError(errorMessage(e)));
  }, [transport, path, account, ready, request]);
  useEffect(refreshPlan, [refreshPlan]);

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
      setError(errorMessage(e));
    } finally {
      setBusy(false);
      loadRuns();
      refreshPlan();
      onDataChanged();
    }
  };
  const remove = async () => {
    try {
      await transport.integrationsRemove(path, "url_inspection");
      setLive(null);
      loadRuns();
      refreshPlan();
      onDataChanged();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const paused = runs.find((r) => resumable(r.state) && r.state !== "running");
  const shown = busy && live?.provider === "url_inspection" ? live : null;
  const setMax = (text: string) => {
    if (!request) return;
    const v = Number.parseInt(text, 10);
    setRequest({ ...request, max: Number.isFinite(v) && v > 0 ? v : null });
  };

  return (
    <section aria-labelledby="inspect-run-heading" className="flex flex-col gap-3 rounded-lg border border-line p-4">
      <div className="flex items-center gap-2">
        <TextSearch aria-hidden className="size-4 text-fg-3" />
        <h2 id="inspect-run-heading" className="m-0 text-base font-semibold text-fg">Index status for this crawl</h2>
      </div>
      {!provider.settings.enabled ? (
        <Notice>Turn on URL Inspection above to ask Google whether chosen pages are indexed, which canonical it chose and whether robots.txt blocked it.</Notice>
      ) : account === null ? (
        <Notice>Connect a Google account that can read Search Console.</Notice>
      ) : (
        <>
          <div className="flex flex-wrap items-end gap-3">
            <label className="flex min-w-60 flex-1 flex-col gap-1 text-sm text-fg-2">
              Property to inspect
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
              URLs to inspect
              <Select value={request?.urls ?? "indexable"} disabled={!request || busy} onChange={(e) => request && setRequest({ ...request, urls: e.target.value })}>
                <option value="indexable">Indexable pages, most clicks first</option>
                <option value="all">All crawled pages</option>
              </Select>
            </label>
            <label className="flex w-32 flex-col gap-1 text-sm text-fg-2">
              At most
              <Input type="number" min={1} value={request?.max ?? ""} disabled={!request || busy} onChange={(e) => setMax(e.target.value)} placeholder="All" />
            </label>
          </div>
          {plan && (
            <div role="status" aria-label="Inspection allowance" className="flex flex-col gap-1 text-sm text-fg-2">
              <span>
                {num(plan.selected)} URLs selected. Today's allowance for this property: {num(plan.remaining_today)} left of {num(plan.allowance)}, resets {new Date(plan.resets_at).toLocaleString()}.
              </span>
              {plan.days_needed > 1 && (
                <Notice kind="warn">
                  This takes {plan.days_needed} days at the daily allowance. The run pauses when the allowance is used up and continues the next day with the URLs still waiting.
                </Notice>
              )}
            </div>
          )}
          <p className="m-0 text-xs text-fg-3">{provider.sends}</p>
          {crawlRunning && <Notice kind="warn">This crawl is still running. Fetch external data when it has finished or paused.</Notice>}
          <div className="flex flex-wrap items-center gap-2">
            {!busy && (
              <Button
                variant="primary"
                disabled={!request || !plan || plan.selected === 0 || crawlRunning}
                onClick={() => request && void go(() => transport.integrationsRunInspection(path, account, request))}
              >
                <Play aria-hidden className="mr-1 size-3.5" />
                Inspect index status
              </Button>
            )}
            {!busy && paused && (
              <Button disabled={crawlRunning} onClick={() => void go(() => transport.integrationsRunResume(path, paused.id))}>
                Resume inspection run {paused.id}
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
        <div role="status" aria-label="Inspection progress" className="flex items-center gap-3 text-sm text-fg-2">
          <Bar value={shown.requested ? shown.done / shown.requested : 0.5} tone="accent" className="w-32" />
          <span>
            {num(shown.done)} of {num(shown.requested)} URLs inspected
          </span>
        </div>
      )}
      {error && <Notice kind="bad" role="alert">{error}</Notice>}

      <RunList
        label="Inspection runs"
        runs={runs}
        summary={(r) => (
          <>
            {num(r.done)} of {num(r.requested)} URLs inspected{r.failed ? `, ${num(r.failed)} failed` : ""}
          </>
        )}
      />

      {runs.length > 0 && !busy && (
        <div>
          <Button size="sm" variant="destructive" onClick={() => void remove()}>
            <Trash2 aria-hidden className="mr-1 size-3.5" />
            Remove URL Inspection data from this crawl
          </Button>
        </div>
      )}
    </section>
  );
}
