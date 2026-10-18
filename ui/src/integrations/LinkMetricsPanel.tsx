// Link metrics (spec 024 US5): Ahrefs, Majestic or Moz numbers for the
// crawled pages. The key lives in the keychain; before anything is sent the
// panel shows the cost in the provider's own units and asks for a go-ahead.

import { useCallback, useEffect, useState } from "react";
import { Link as LinkIcon, Play } from "lucide-react";
import { Badge, Button, Input, Modal, Notice, NumberInput, Select } from "../components/ui";
import type { LinkEstimate } from "../generated/LinkEstimate";
import type { LinkRequest } from "../generated/LinkRequest";
import type { MetricInfo } from "../generated/MetricInfo";
import type { ProviderStatus } from "../generated/ProviderStatus";
import type { RunProgress } from "../generated/RunProgress";
import type { Status } from "../generated/Status";
import type { LinkUrlSet } from "../generated/LinkUrlSet";
import type { Transport } from "../transport";

type LinkProvider = "ahrefs" | "majestic" | "moz";
const PROVIDERS: LinkProvider[] = ["ahrefs", "majestic", "moz"];
const n = (v: number) => v.toLocaleString();

function message(e: unknown) {
  return e instanceof Error ? e.message : String(e);
}

export function LinkMetricsPanel({
  transport,
  path,
  status,
  crawlRunning,
  onChanged,
  onDataChanged,
}: {
  transport: Transport;
  path: string;
  status: Status;
  crawlRunning: boolean;
  /** The connections or settings changed. */
  onChanged: () => void;
  onDataChanged: () => void;
}) {
  const [id, setId] = useState<LinkProvider>("ahrefs");
  const [metrics, setMetrics] = useState<MetricInfo[]>([]);
  const [chosen, setChosen] = useState<string[]>([]);
  const [urlSet, setUrlSet] = useState<LinkUrlSet>("indexable");
  const [cap, setCap] = useState<number | null>(null);
  const [keyLabel, setKeyLabel] = useState("");
  const [key, setKey] = useState("");
  const [estimate, setEstimate] = useState<LinkEstimate | null>(null);
  const [result, setResult] = useState<RunProgress | null>(null);
  const [unit, setUnit] = useState("units");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const provider = status.providers.find((p) => p.provider === id) as ProviderStatus;
  const connection = status.connections.find((c) => c.kind === id) ?? null;

  useEffect(() => {
    let off = false;
    setEstimate(null);
    setResult(null);
    transport.integrationsLinkMetrics(id).then((m) => {
      if (off) return;
      setMetrics(m);
      setChosen(m.map((x) => x.id));
    }, () => {});
    return () => {
      off = true;
    };
  }, [transport, id]);

  const request = useCallback(
    (withCap: number | null): LinkRequest => ({ provider: id, url_set: urlSet, metrics: chosen, cap: withCap, refresh: false }),
    [id, urlSet, chosen],
  );

  const saveKey = async () => {
    setError(null);
    try {
      await transport.integrationsSaveKey(id, keyLabel.trim(), key.trim());
      setKey("");
      onChanged();
    } catch (e) {
      setError(message(e));
    }
  };
  const turnOn = async () => {
    try {
      await transport.integrationsSettingsSave(id, { ...provider.settings, enabled: true });
      onChanged();
    } catch (e) {
      setError(message(e));
    }
  };
  const askCost = async () => {
    setError(null);
    setResult(null);
    try {
      const e = await transport.integrationsLinkEstimate(path, connection?.label ?? null, request(null));
      setEstimate(e);
      setUnit(e.unit);
      setCap(e.default_cap);
    } catch (e) {
      setError(message(e));
    }
  };
  const start = async () => {
    if (!estimate) return;
    const withCap = cap ?? estimate.default_cap;
    setEstimate(null);
    setBusy(true);
    setError(null);
    try {
      setResult(await transport.integrationsRunLinks(path, connection?.label ?? null, request(withCap)));
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
      onDataChanged();
    }
  };

  const ready = provider.settings.enabled && connection !== null;
  const stopped = result !== null && result.state === "finished" && result.done < result.requested;

  return (
    <section aria-labelledby="links-heading" className="flex flex-col gap-3 rounded-lg border border-line p-4">
      <div className="flex items-center gap-2">
        <LinkIcon aria-hidden className="size-4 text-fg-3" />
        <h2 id="links-heading" className="m-0 text-base font-semibold text-fg">Link metrics for this crawl</h2>
      </div>
      <label className="flex max-w-60 flex-col gap-1 text-sm text-fg-2">
        Provider
        <Select value={id} disabled={busy} onChange={(e) => setId(e.target.value as LinkProvider)}>
          {PROVIDERS.map((p) => (
            <option key={p} value={p}>
              {status.providers.find((s) => s.provider === p)?.label ?? p}
            </option>
          ))}
        </Select>
      </label>
      <p className="m-0 text-xs text-fg-3">{provider.sends}</p>

      {connection === null ? (
        <form
          aria-label={`${provider.label} key`}
          className="flex flex-col gap-2 rounded-md border border-line p-3"
          onSubmit={(e) => {
            e.preventDefault();
            void saveKey();
          }}
        >
          <label className="flex flex-col gap-1 text-sm text-fg-2">
            Name for this key
            <Input value={keyLabel} onChange={(e) => setKeyLabel(e.target.value)} placeholder="Agency account" />
          </label>
          <label className="flex flex-col gap-1 text-sm text-fg-2">
            {id === "moz" ? "Access ID and secret, as ACCESS_ID:SECRET" : "API key"}
            <Input type="password" mono value={key} onChange={(e) => setKey(e.target.value)} autoComplete="off" />
          </label>
          <div>
            <Button type="submit" variant="primary" size="sm" disabled={!keyLabel.trim() || !key.trim()}>
              Save to keychain
            </Button>
          </div>
        </form>
      ) : (
        <p className="m-0 flex items-center gap-2 text-sm text-fg-2">
          Key <Badge>{connection.label}</Badge> saved in your keychain.
        </p>
      )}

      {connection !== null && !provider.settings.enabled && (
        <div className="flex items-center gap-2">
          <Notice>{provider.label} is off.</Notice>
          <Button size="sm" onClick={() => void turnOn()}>Turn on {provider.label}</Button>
        </div>
      )}

      {ready && (
        <>
          <fieldset className="m-0 flex flex-wrap gap-x-4 gap-y-1 border-0 p-0">
            <legend className="mb-1 p-0 text-sm text-fg-2">Metrics</legend>
            {metrics.map((m) => (
              <label key={m.id} className="flex items-center gap-1.5 text-sm text-fg">
                <input
                  type="checkbox"
                  checked={chosen.includes(m.id)}
                  disabled={busy}
                  onChange={(e) => setChosen(e.target.checked ? [...chosen, m.id] : chosen.filter((x) => x !== m.id))}
                />
                {m.label}
              </label>
            ))}
          </fieldset>
          <label className="flex max-w-60 flex-col gap-1 text-sm text-fg-2">
            Pages
            <Select value={urlSet} disabled={busy} onChange={(e) => setUrlSet(e.target.value as LinkUrlSet)}>
              <option value="indexable">Indexable pages</option>
              <option value="all">All internal pages</option>
            </Select>
          </label>
          {crawlRunning && <Notice kind="warn">This crawl is still running. Fetch external data when it has finished or paused.</Notice>}
          <div>
            <Button variant="primary" disabled={busy || crawlRunning || chosen.length === 0} onClick={() => void askCost()}>
              <Play aria-hidden className="mr-1 size-3.5" />
              Estimate cost
            </Button>
          </div>
        </>
      )}

      <Modal
        open={estimate !== null}
        onOpenChange={(o) => !o && setEstimate(null)}
        title="Confirm link metrics run"
        description={estimate ? `${provider.label} charges your plan for this run.` : undefined}
      >
        {estimate && (
          <>
            <p className="m-0 text-sm text-fg">
              {n(estimate.urls)} pages ({n(estimate.cached)} already cached) will use about{" "}
              <strong>{n(estimate.units)} {estimate.unit}</strong> in {n(estimate.requests)} {estimate.requests === 1 ? "request" : "requests"}.
              The first batch re-checks this estimate.
            </p>
            <label className="flex flex-col gap-1 text-sm text-fg-2">
              Stop after ({estimate.unit})
              <NumberInput aria-label={`Cap in ${estimate.unit}`} className="w-32" optional value={cap} min={0} onChange={setCap} />
            </label>
            <p className="m-0 text-xs text-fg-3">
              The run stops when it reaches this cap, and keeps what it fetched. The default is the estimate plus 10%.
            </p>
            <div className="flex justify-end gap-2">
              <Button variant="quiet" onClick={() => setEstimate(null)}>Cancel</Button>
              <Button variant="primary" onClick={() => void start()}>Start run</Button>
            </div>
          </>
        )}
      </Modal>

      {busy && <p role="status" className="m-0 text-sm text-fg-2">Fetching link metrics…</p>}
      {result && (
        <Notice kind={result.state === "finished" && !stopped ? "info" : "warn"} role="status">
          {n(result.done)} of {n(result.requested)} pages fetched, {result.usage !== null ? `${n(result.usage)} ${unit} used` : "usage not reported"}.
          {result.message ? ` ${result.message}` : ""}
        </Notice>
      )}
      {error && <Notice kind="bad" role="alert">{error}</Notice>}
    </section>
  );
}
