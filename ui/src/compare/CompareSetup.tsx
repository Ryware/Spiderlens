// Choosing the two crawls and the URL mapping before a comparison runs
// (spec 012 story 3 and 4, FR-001..FR-010).

import { useEffect, useState } from "react";
import { ArrowUpDown, Plus, X } from "lucide-react";
import type { CompareSettings } from "../generated/CompareSettings";
import type { CrawlInfo } from "../generated/CrawlInfo";
import type { MapRule } from "../generated/MapRule";
import type { MappingTest } from "../generated/MappingTest";
import type { Setup } from "../generated/Setup";
import type { ScheduleBaseline, Transport } from "../transport";
import { Badge, Button, Input, Notice, Sheet, SheetClose, Switch, Textarea, type Tone } from "../components/ui";
import { fileName } from "./useComparison";

function completeness(i: CrawlInfo): [Tone, string] {
  if (i.snapshot) return ["info", "Still crawling: a snapshot is compared"];
  if (i.url_limit_reached) return ["warn", "Stopped at the URL limit"];
  if (!i.complete) return ["warn", i.status === "paused" ? "Paused" : "Stopped before the end"];
  return ["ok", "Complete"];
}

function CrawlCard({ label, info }: { label: string; info: CrawlInfo | null }) {
  if (!info) return null;
  const [tone, text] = completeness(info);
  return (
    <div aria-label={`${label} crawl`} className="flex flex-col gap-1 rounded-lg border border-line bg-surface-2 px-3 py-2 text-sm">
      <div className="flex items-center gap-2">
        <span className="font-semibold">{label}</span>
        <Badge tone={tone}>{text}</Badge>
      </div>
      <div className="truncate text-fg-2" title={info.start_url ?? undefined}>
        {info.start_url ?? "No start URL recorded"}
      </div>
      <div className="text-fg-3 tabular">
        {info.started_at ? new Date(info.started_at).toLocaleString() : "Date not recorded"} · {info.urls.toLocaleString()} URLs
      </div>
    </div>
  );
}

function FileRow({ label, path, onBrowse }: { label: string; path: string | null; onBrowse: () => void }) {
  return (
    <div className="flex items-center gap-2">
      <span className="w-20 shrink-0 text-sm text-fg-2">{label}</span>
      <span aria-label={`${label} file`} className="min-w-0 flex-1 truncate rounded-md border border-line bg-surface-2 px-2 py-1 font-code text-sm" title={path ?? undefined}>
        {path ? fileName(path) : <span className="text-fg-3">No crawl chosen</span>}
      </span>
      <Button size="sm" onClick={onBrowse} aria-label={`Choose the ${label.toLowerCase()} crawl`}>
        Choose…
      </Button>
    </div>
  );
}

export function CompareSetup({
  open,
  onOpenChange,
  transport,
  initialCurrent,
  onStart,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  transport: Transport;
  /** The open crawl, compared as the current one by default. */
  initialCurrent: string | null;
  onStart: (previous: string, current: string, settings: CompareSettings) => void;
}) {
  const [previous, setPrevious] = useState<string | null>(null);
  const [current, setCurrent] = useState<string | null>(null);
  const [setup, setSetup] = useState<Setup | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [rules, setRules] = useState<MapRule[]>([]);
  const [samples, setSamples] = useState("");
  const [tests, setTests] = useState<MappingTest[] | null>(null);
  // The previous run of the current crawl's schedule, when it is a scheduled run (FR-002a).
  const [baseline, setBaseline] = useState<ScheduleBaseline | null>(null);
  const [baselineError, setBaselineError] = useState<string | null>(null);

  // Opening the sheet starts from the open crawl and suggests the previous
  // crawl of the same site (FR-001).
  useEffect(() => {
    if (!open) return;
    setCurrent(initialCurrent);
    setPrevious(null);
    setRules([]);
    setTests(null);
    if (initialCurrent) transport.compareSuggest(initialCurrent).then((p) => p && setPrevious((x) => x ?? p), () => {});
  }, [open, initialCurrent, transport]);

  useEffect(() => {
    setSetup(null);
    setError(null);
    if (!previous || !current) return;
    let off = false;
    transport.compareSetup(previous, current).then(
      (s) => !off && setSetup(s),
      (e) => !off && setError(String(e)),
    );
    return () => {
      off = true;
    };
  }, [transport, previous, current]);

  useEffect(() => {
    setBaseline(null);
    setBaselineError(null);
    if (!open || !current) return;
    let off = false;
    transport.compareScheduleBaseline(current).then(
      (b) => !off && setBaseline(b),
      (e) => !off && setBaselineError(String(e)),
    );
    return () => {
      off = true;
    };
  }, [open, transport, current]);

  const browse = async (set: (p: string) => void) => {
    const p = await transport.pickCrawlFile();
    if (p) set(p);
  };
  const setRule = (i: number, patch: Partial<MapRule>) => setRules((rs) => rs.map((r, j) => (j === i ? { ...r, ...patch } : r)));
  const test = async () => {
    if (!previous || !current) return;
    try {
      setTests(await transport.compareTestMapping(previous, current, rules, samples.split("\n"), samples.trim() ? 0 : 10));
    } catch (e) {
      setTests(null);
      setError(String(e));
    }
  };
  const ready = previous !== null && current !== null && setup !== null && error === null;
  const usedRules = rules.filter((r) => r.find !== "");

  return (
    <Sheet open={open} onOpenChange={onOpenChange} title="Compare crawls" className="w-180 sheet-max-w">
      <header className="flex items-center gap-2 border-b border-line px-4 py-3">
        <div aria-hidden className="text-md font-semibold">Compare crawls</div>
        <span className="text-sm text-fg-3">What changed between two crawls of a site. Neither file is changed.</span>
      </header>
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-4 py-4">
        <div className="flex flex-col gap-2">
          <FileRow label="Previous" path={previous} onBrowse={() => void browse(setPrevious)} />
          <div className="flex pl-22">
            <Button
              size="sm"
              variant="quiet"
              disabled={!previous && !current}
              onClick={() => {
                setPrevious(current);
                setCurrent(previous);
              }}
            >
              <ArrowUpDown aria-hidden className="size-3.5" /> Swap
            </Button>
          </div>
          <FileRow label="Current" path={current} onBrowse={() => void browse(setCurrent)} />
          {baseline && (
            <div className="flex pl-22">
              <Button size="sm" disabled={previous === baseline.path} onClick={() => setPrevious(baseline.path)}>
                Previous run of “{baseline.schedule}” (run {baseline.run})
              </Button>
            </div>
          )}
          {baselineError && <Notice kind="info">{baselineError}</Notice>}
        </div>

        {error && (
          <Notice kind="bad" role="alert">
            {error}
          </Notice>
        )}

        {setup && (
          <div className="grid grid-cols-2 gap-2">
            <CrawlCard label="Previous" info={setup.previous} />
            <CrawlCard label="Current" info={setup.current} />
          </div>
        )}

        {setup?.disk_warning && <Notice kind="warn">{setup.disk_warning}</Notice>}

        {setup && setup.config_diffs.length > 0 && (
          <section aria-label="Configuration differences" className="flex flex-col gap-1">
            <Notice kind="warn">The crawls were made with different settings, so some differences come from the settings and not the site.</Notice>
            <ul className="m-0 flex list-none flex-col gap-0.5 p-0 pl-5 text-sm">
              {setup.config_diffs.map((d) => (
                <li key={d.field}>
                  <span className="text-fg-2">{d.label}:</span> {d.previous} → {d.current}
                </li>
              ))}
            </ul>
          </section>
        )}

        <section aria-label="URL mapping" className="flex flex-col gap-2">
          <div className="flex items-center gap-2">
            <h3 className="m-0 text-sm font-semibold">URL mapping</h3>
            <span className="text-sm text-fg-3">For a migration: rewrite previous URLs before matching. The first matching rule applies.</span>
          </div>
          {setup?.suggested_mapping && rules.length === 0 && (
            <div className="flex items-center gap-2">
              <Notice kind="info" className="flex-1">
                The crawls start on different sites. Map {setup.suggested_mapping.find} to {setup.suggested_mapping.replace}?
              </Notice>
              <Button size="sm" onClick={() => setRules([setup.suggested_mapping!])}>
                Use this rule
              </Button>
            </div>
          )}
          {rules.map((r, i) => (
            <div key={i} className="flex items-center gap-2" aria-label={`Rule ${i + 1}`} role="group">
              <Input aria-label="Find" mono className="min-w-0 flex-1" value={r.find} placeholder="https://old.example.com/" onChange={(e) => setRule(i, { find: e.target.value })} />
              <span aria-hidden className="text-fg-3">→</span>
              <Input aria-label="Replace with" mono className="min-w-0 flex-1" value={r.replace} placeholder="https://www.example.com/" onChange={(e) => setRule(i, { replace: e.target.value })} />
              <label className="flex items-center gap-1 text-sm text-fg-2">
                <Switch label="Regular expression" checked={r.regex} onChange={(v) => setRule(i, { regex: v })} />
                Regex
              </label>
              <Button size="sm" icon variant="quiet" aria-label={`Remove rule ${i + 1}`} onClick={() => setRules((rs) => rs.filter((_, j) => j !== i))}>
                <X aria-hidden className="size-3.5" />
              </Button>
            </div>
          ))}
          <div>
            <Button size="sm" variant="quiet" onClick={() => setRules((rs) => [...rs, { find: "", replace: "", regex: false }])}>
              <Plus aria-hidden className="size-3.5" /> Add rule
            </Button>
          </div>
          {rules.length > 0 && (
            <div className="flex flex-col gap-2">
              <Textarea aria-label="Test URLs" rows={3} value={samples} placeholder="Previous URLs to test, one per line (empty: the first 10 of the previous crawl)" onChange={(e) => setSamples(e.target.value)} />
              <div>
                <Button size="sm" disabled={!previous || !current} onClick={() => void test()}>
                  Test mapping
                </Button>
              </div>
              {tests && (
                <ul aria-label="Mapping test results" className="m-0 flex list-none flex-col gap-1 p-0 text-sm">
                  {tests.map((t) => (
                    <li key={t.url} className="flex min-w-0 items-center gap-2">
                      <span className="min-w-0 flex-1 truncate font-code" title={t.url}>
                        {t.url}
                      </span>
                      <span aria-hidden className="text-fg-3">→</span>
                      <span className="min-w-0 flex-1 truncate font-code" title={t.mapped}>
                        {t.mapped}
                      </span>
                      <Badge tone={t.exists_in_current ? "ok" : "warn"}>{t.exists_in_current ? "In current crawl" : "Not in current crawl"}</Badge>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </section>
      </div>
      <footer className="flex items-center justify-end gap-2 border-t border-line px-4 py-3">
        <SheetClose render={<Button />}>Cancel</SheetClose>
        <Button
          variant="primary"
          disabled={!ready}
          onClick={() => {
            onStart(previous!, current!, { mapping: usedRules, elements: [], issue_settings: "current" });
            onOpenChange(false);
          }}
        >
          Compare
        </Button>
      </footer>
    </Sheet>
  );
}
