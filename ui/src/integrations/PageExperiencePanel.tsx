// Page experience (spec 024 US3): Core Web Vitals field data from the Chrome
// UX Report and lab tests from PageSpeed Insights for the open crawl. One
// Google API key, kept in the system keychain, serves both. Both run after
// the crawl, show an estimate before they start, and can be paused and
// resumed.

import { useCallback, useEffect, useMemo, useState } from "react";
import { Gauge, KeyRound, Pause, Play, Square, Trash2 } from "lucide-react";
import { Badge, Bar, Button, Input, Notice, Select } from "../components/ui";
import type { Connection } from "../generated/Connection";
import type { FieldRequest } from "../generated/FieldRequest";
import type { FormFactor } from "../generated/FormFactor";
import type { LabRequest } from "../generated/LabRequest";
import type { PerfEstimate } from "../generated/PerfEstimate";
import type { ProviderStatus } from "../generated/ProviderStatus";
import type { RunProgress } from "../generated/RunProgress";
import type { RunState } from "../generated/RunState";
import type { StrategyChoice } from "../generated/StrategyChoice";
import type { UrlSet } from "../generated/UrlSet";
import type { Transport } from "../transport";

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
const resumable = (s: RunState) => s === "paused_user" || s === "paused_quota" || s === "queued";
const n = (v: number | null) => (v ?? 0).toLocaleString();

type PageMode = "all" | "indexable" | "prefix";

function message(e: unknown) {
  return e instanceof Error ? e.message : String(e);
}

/** "4 min", "2 h 19 min": how long a run takes. */
export function formatDuration(secs: number): string {
  if (secs < 90) return `${secs} s`;
  if (secs < 5400) return `${Math.ceil(secs / 60)} min`;
  return `${Math.floor(secs / 3600)} h ${Math.ceil((secs % 3600) / 60)} min`;
}

function urlSet(mode: PageMode, prefix: string): UrlSet {
  return mode === "prefix" ? { kind: "prefix", prefix } : { kind: mode };
}

function maxOf(text: string): number | null {
  const v = Number.parseInt(text, 10);
  return Number.isFinite(v) && v > 0 ? v : null;
}

function Estimate({ label, e }: { label: string; e: PerfEstimate | null }) {
  if (!e) return null;
  return (
    <p role="status" aria-label={label} className="m-0 text-sm text-fg-2">
      Estimate: {n(e.requests)} requests for {n(e.urls)} pages, about {formatDuration(e.duration_secs)} at up to {e.per_minute} a minute and {e.concurrency} at a time.
      {e.share_of_allowance !== null && ` That is ${Math.round(e.share_of_allowance * 100)}% of a day's allowance.`}
      {e.days_needed > 1 && ` It needs ${e.days_needed} days of the allowance; the run pauses at the limit and you resume it later.`}
    </p>
  );
}

export function PageExperiencePanel({
  transport,
  path,
  field,
  lab,
  keys,
  crawlRunning,
  onChanged,
  onDataChanged,
}: {
  transport: Transport;
  path: string;
  field: ProviderStatus;
  lab: ProviderStatus;
  /** Saved connections; the Google API keys are the ones of field data and lab tests. */
  keys: Connection[];
  crawlRunning: boolean;
  /** The saved keys or settings changed. */
  onChanged: () => void;
  onDataChanged: () => void;
}) {
  const [keyName, setKeyName] = useState("Google API key");
  const [keyValue, setKeyValue] = useState("");
  const [phone, setPhone] = useState(true);
  const [desktop, setDesktop] = useState(true);
  const [fieldPages, setFieldPages] = useState<PageMode>("all");
  const [fieldMax, setFieldMax] = useState("");
  const [strategy, setStrategy] = useState<StrategyChoice>("mobile");
  const [labPages, setLabPages] = useState<PageMode>("indexable");
  const [labPrefix, setLabPrefix] = useState("");
  const [labMax, setLabMax] = useState("50");
  const [fieldEstimate, setFieldEstimate] = useState<PerfEstimate | null>(null);
  const [labEstimate, setLabEstimate] = useState<PerfEstimate | null>(null);
  const [runs, setRuns] = useState<RunProgress[]>([]);
  const [live, setLive] = useState<RunProgress | null>(null);
  // Which job is running, so its own block shows Pause and Cancel.
  const [running, setRunning] = useState<"field_data" | "lab_tests" | null>(null);
  const busy = running !== null;
  const [error, setError] = useState<string | null>(null);

  const saved = [...new Set(keys.filter((c) => c.kind === "field_data" || c.kind === "lab_tests").map((c) => c.label))];
  const fieldReady = field.settings.enabled && field.connections.length > 0;
  const labReady = lab.settings.enabled && lab.connections.length > 0;

  const fieldRequest = useMemo<FieldRequest | null>(() => {
    const devices: FormFactor[] = [...(phone ? (["phone"] as const) : []), ...(desktop ? (["desktop"] as const) : [])];
    return devices.length ? { devices, urls: urlSet(fieldPages, ""), max: maxOf(fieldMax), fresh: false } : null;
  }, [phone, desktop, fieldPages, fieldMax]);
  const labRequest = useMemo<LabRequest | null>(
    () => (labPages === "prefix" && !labPrefix.trim() ? null : { strategy, urls: urlSet(labPages, labPrefix.trim()), max: maxOf(labMax), fresh: false }),
    [strategy, labPages, labPrefix, labMax],
  );

  const loadRuns = useCallback(() => {
    transport.integrationsRuns(path).then(setRuns, () => {});
  }, [transport, path]);
  useEffect(loadRuns, [loadRuns]);

  useEffect(() => {
    if (!fieldRequest) {
      setFieldEstimate(null);
      return;
    }
    let off = false;
    transport.integrationsEstimateFieldData(path, fieldRequest).then(
      (e) => !off && setFieldEstimate(e),
      () => !off && setFieldEstimate(null),
    );
    return () => {
      off = true;
    };
  }, [transport, path, fieldRequest]);
  useEffect(() => {
    if (!labRequest) {
      setLabEstimate(null);
      return;
    }
    let off = false;
    transport.integrationsEstimateLabTests(path, labRequest).then(
      (e) => !off && setLabEstimate(e),
      () => !off && setLabEstimate(null),
    );
    return () => {
      off = true;
    };
  }, [transport, path, labRequest]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let off = false;
    transport.onIntegrationsProgress((p) => setLive(p)).then((u) => (off ? u() : (unlisten = u)));
    return () => {
      off = true;
      unlisten?.();
    };
  }, [transport]);

  const go = async (job: "field_data" | "lab_tests", start: () => Promise<RunProgress>) => {
    setError(null);
    setRunning(job);
    try {
      setLive(await start());
    } catch (e) {
      setError(message(e));
    } finally {
      setRunning(null);
      loadRuns();
      onDataChanged();
    }
  };
  const saveKey = async () => {
    setError(null);
    try {
      await transport.integrationsSaveGoogleKey(keyName.trim(), keyValue.trim());
      setKeyValue("");
      onChanged();
    } catch (e) {
      setError(message(e));
    }
  };
  const forgetKey = async (label: string) => {
    try {
      await transport.integrationsForget("field_data", label);
      await transport.integrationsForget("lab_tests", label);
      onChanged();
    } catch (e) {
      setError(message(e));
    }
  };
  const remove = async (provider: "field_data" | "lab_tests") => {
    try {
      await transport.integrationsRemove(path, provider);
      setLive(null);
      loadRuns();
      onDataChanged();
    } catch (e) {
      setError(message(e));
    }
  };

  const mine = runs.filter((r) => r.provider === "field_data" || r.provider === "lab_tests");
  const paused = (p: "field_data" | "lab_tests") => mine.find((r) => r.provider === p && resumable(r.state));
  const shown = busy ? live : null;
  const controls = (
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
  );

  return (
    <section aria-labelledby="px-heading" className="flex flex-col gap-4 rounded-lg border border-line p-4">
      <div className="flex items-center gap-2">
        <Gauge aria-hidden className="size-4 text-fg-3" />
        <h2 id="px-heading" className="m-0 text-base font-semibold text-fg">Page experience for this crawl</h2>
      </div>
      <p className="m-0 text-xs text-fg-3">
        Real-user Core Web Vitals from the Chrome UX Report and lab tests from PageSpeed Insights. Both use your own Google API key, which is kept in your system keychain, never in crawl files or exports. One key serves both when both APIs are enabled for its Google Cloud project.
      </p>

      <div className="flex flex-col gap-2">
        <h3 className="m-0 text-sm font-semibold text-fg">Google API key</h3>
        {saved.length > 0 && (
          <ul aria-label="Saved API keys" className="m-0 flex list-none flex-col gap-1 p-0">
            {saved.map((label) => (
              <li key={label} className="flex items-center gap-2 rounded-md bg-surface-2 px-3 py-2 text-sm">
                <KeyRound aria-hidden className="size-4 text-fg-3" />
                <span className="min-w-0 flex-1 truncate text-fg">{label}</span>
                <Button size="sm" variant="destructive" onClick={() => void forgetKey(label)} aria-label={`Forget key ${label}`}>
                  Forget
                </Button>
              </li>
            ))}
          </ul>
        )}
        <form
          aria-label="Save a Google API key"
          className="flex flex-wrap items-end gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            void saveKey();
          }}
        >
          <label className="flex min-w-40 flex-col gap-1 text-sm text-fg-2">
            Key name
            <Input value={keyName} onChange={(e) => setKeyName(e.target.value)} />
          </label>
          <label className="flex min-w-60 flex-1 flex-col gap-1 text-sm text-fg-2">
            API key
            <Input type="password" mono autoComplete="off" value={keyValue} onChange={(e) => setKeyValue(e.target.value)} />
          </label>
          <Button type="submit" size="sm" variant="primary" disabled={!keyName.trim() || !keyValue.trim()}>
            Save key to keychain
          </Button>
        </form>
      </div>

      <div className="flex flex-col gap-3 border-t border-line pt-3">
        <h3 className="m-0 text-sm font-semibold text-fg">Field data (Chrome UX Report)</h3>
        {!field.settings.enabled ? (
          <Notice>Turn on Chrome UX Report (field data) above to fetch the 75th percentile of LCP, INP, CLS, FCP and TTFB for each page.</Notice>
        ) : !fieldReady ? (
          <Notice>Save a Google API key to fetch field data.</Notice>
        ) : (
          <>
            <div className="flex flex-wrap items-end gap-4">
              <fieldset className="m-0 flex items-center gap-3 border-0 p-0">
                <legend className="mb-1 p-0 text-sm text-fg-2">Devices</legend>
                <label className="flex items-center gap-1.5 text-sm text-fg">
                  <input type="checkbox" checked={phone} disabled={busy} onChange={(e) => setPhone(e.target.checked)} />
                  Phone
                </label>
                <label className="flex items-center gap-1.5 text-sm text-fg">
                  <input type="checkbox" checked={desktop} disabled={busy} onChange={(e) => setDesktop(e.target.checked)} />
                  Desktop
                </label>
              </fieldset>
              <label className="flex flex-col gap-1 text-sm text-fg-2">
                Pages
                <Select value={fieldPages} disabled={busy} onChange={(e) => setFieldPages(e.target.value as PageMode)}>
                  <option value="all">All HTML pages</option>
                  <option value="indexable">Indexable pages</option>
                </Select>
              </label>
              <label className="flex flex-col gap-1 text-sm text-fg-2">
                Most pages
                <Input inputMode="numeric" className="w-28" placeholder="No limit" value={fieldMax} disabled={busy} onChange={(e) => setFieldMax(e.target.value)} />
              </label>
            </div>
            <p className="m-0 text-xs text-fg-3">
              {field.sends} A page with too little traffic shows the whole site's numbers, labelled as origin data. Field data covers the last 28 days and updates daily.
            </p>
            <Estimate label="Field data estimate" e={fieldEstimate} />
            {crawlRunning && <Notice kind="warn">This crawl is still running. Fetch external data when it has finished or paused.</Notice>}
            <div className="flex flex-wrap items-center gap-2">
              {!busy && (
                <Button
                  variant="primary"
                  disabled={!fieldRequest || crawlRunning}
                  onClick={() => fieldRequest && void go("field_data", () => transport.integrationsRunFieldData(path, null, fieldRequest))}
                >
                  <Play aria-hidden className="mr-1 size-3.5" />
                  Fetch field data
                </Button>
              )}
              {!busy && paused("field_data") && (
                <Button disabled={crawlRunning} onClick={() => void go("field_data", () => transport.integrationsRunResume(path, paused("field_data")!.id))}>
                  Resume field data run {paused("field_data")!.id}
                </Button>
              )}
              {running === "field_data" && controls}
            </div>
          </>
        )}
      </div>

      <div className="flex flex-col gap-3 border-t border-line pt-3">
        <h3 className="m-0 text-sm font-semibold text-fg">Lab tests (PageSpeed Insights)</h3>
        {!lab.settings.enabled ? (
          <Notice>Turn on PageSpeed Insights (lab tests) above to test chosen pages for performance, accessibility, best practices and SEO.</Notice>
        ) : !labReady ? (
          <Notice>Save a Google API key to run lab tests.</Notice>
        ) : (
          <>
            <div className="flex flex-wrap items-end gap-4">
              <label className="flex flex-col gap-1 text-sm text-fg-2">
                Device
                <Select value={strategy} disabled={busy} onChange={(e) => setStrategy(e.target.value as StrategyChoice)}>
                  <option value="mobile">Mobile</option>
                  <option value="desktop">Desktop</option>
                  <option value="both">Mobile and desktop</option>
                </Select>
              </label>
              <label className="flex flex-col gap-1 text-sm text-fg-2">
                Test pages
                <Select value={labPages} disabled={busy} onChange={(e) => setLabPages(e.target.value as PageMode)}>
                  <option value="indexable">Indexable pages</option>
                  <option value="all">All HTML pages</option>
                  <option value="prefix">Pages starting with…</option>
                </Select>
              </label>
              {labPages === "prefix" && (
                <label className="flex min-w-60 flex-1 flex-col gap-1 text-sm text-fg-2">
                  Address starts with
                  <Input mono value={labPrefix} disabled={busy} placeholder="https://example.com/blog/" onChange={(e) => setLabPrefix(e.target.value)} />
                </label>
              )}
              <label className="flex flex-col gap-1 text-sm text-fg-2">
                Most pages to test
                <Input inputMode="numeric" className="w-28" placeholder="No limit" value={labMax} disabled={busy} onChange={(e) => setLabMax(e.target.value)} />
              </label>
            </div>
            <p className="m-0 text-xs text-fg-3">
              {lab.sends} Each test takes tens of seconds, so test the pages that matter. Pages closest to the start page go first.
            </p>
            <Estimate label="Lab test estimate" e={labEstimate} />
            <div className="flex flex-wrap items-center gap-2">
              {!busy && (
                <Button
                  variant="primary"
                  disabled={!labRequest || crawlRunning}
                  onClick={() => labRequest && void go("lab_tests", () => transport.integrationsRunLabTests(path, null, labRequest))}
                >
                  <Play aria-hidden className="mr-1 size-3.5" />
                  Run lab tests
                </Button>
              )}
              {!busy && paused("lab_tests") && (
                <Button disabled={crawlRunning} onClick={() => void go("lab_tests", () => transport.integrationsRunResume(path, paused("lab_tests")!.id))}>
                  Resume lab test run {paused("lab_tests")!.id}
                </Button>
              )}
              {running === "lab_tests" && controls}
            </div>
          </>
        )}
      </div>

      {shown && (
        <div role="status" aria-label="Page experience progress" className="flex items-center gap-3 text-sm text-fg-2">
          <Bar value={shown.requested ? (shown.done + shown.failed) / shown.requested : 0.5} tone="accent" className="w-32" />
          <span>
            {n(shown.done + shown.failed)} of {n(shown.requested)} pages
          </span>
        </div>
      )}
      {error && <Notice kind="bad" role="alert">{error}</Notice>}

      {mine.length > 0 && (
        <ul aria-label="Page experience runs" className="m-0 flex list-none flex-col gap-1 p-0">
          {mine.map((r) => (
            <li key={r.id} data-run={r.id} className="flex flex-wrap items-center gap-2 text-sm text-fg-2">
              <Badge tone={STATE_TONE[r.state]}>{STATE_LABEL[r.state]}</Badge>
              <span className="text-fg">{r.provider === "field_data" ? "Field data" : "Lab tests"}</span>
              <span>
                {n(r.done)} of {n(r.requested)} pages{r.failed ? `, ${n(r.failed)} failed` : ""}
              </span>
              {r.state === "paused_quota" && r.resume_at && <span className="text-warn">Resume after {new Date(r.resume_at).toLocaleString()}</span>}
              {r.message && r.state !== "finished" && <span className="text-fg-3">{r.message}</span>}
            </li>
          ))}
        </ul>
      )}

      {!busy && (
        <div className="flex flex-wrap gap-2">
          {mine.some((r) => r.provider === "field_data") && (
            <Button size="sm" variant="destructive" onClick={() => void remove("field_data")}>
              <Trash2 aria-hidden className="mr-1 size-3.5" />
              Remove field data from this crawl
            </Button>
          )}
          {mine.some((r) => r.provider === "lab_tests") && (
            <Button size="sm" variant="destructive" onClick={() => void remove("lab_tests")}>
              <Trash2 aria-hidden className="mr-1 size-3.5" />
              Remove lab test results from this crawl
            </Button>
          )}
        </div>
      )}
    </section>
  );
}
