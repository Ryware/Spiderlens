// Settings › Custom rules (spec 013 US1, US2, US4, US5): extractors and
// searches with the engine's errors inline, rule files, a test on one page,
// and running the rules on the open crawl's stored HTML.

import { useState } from "react";
import { FileDown, FileUp, Play, Plus, X } from "lucide-react";
import type { CustomRules } from "../generated/CustomRules";
import type { ElementKind } from "../generated/ElementKind";
import type { ExtractMode } from "../generated/ExtractMode";
import type { Extractor } from "../generated/Extractor";
import type { ExtractorKind } from "../generated/ExtractorKind";
import type { Issue } from "../generated/Issue";
import type { Search } from "../generated/Search";
import type { SearchCondition } from "../generated/SearchCondition";
import type { SearchTarget } from "../generated/SearchTarget";
import type { CrawlSetup } from "../crawl/useCrawlSetup";
import type { RuleTestOutcome, Transport } from "../transport";
import { Badge, Bar, Button, Input, Notice, Select, Switch } from "../components/ui";
import { runOutcomeText, useCustomRun } from "../results/useCustomRun";
import { Group, IssueList, issuesAt, Row } from "./fields";

const KINDS: { value: ExtractorKind; label: string; placeholder: string }[] = [
  { value: "css", label: "CSS selector", placeholder: ".product-price" },
  { value: "xpath", label: "XPath", placeholder: "//meta[@itemprop='sku']/@content" },
  { value: "regex", label: "Regular expression", placeholder: "data-sku=\"([^\"]+)\"" },
  { value: "json_ld", label: "JSON-LD path", placeholder: "offers.price" },
];

const MODES: { value: ExtractMode; label: string; xpathOnly?: boolean }[] = [
  { value: "text", label: "Text" },
  { value: "inner_html", label: "Inner HTML" },
  { value: "outer_html", label: "Outer HTML" },
  { value: "attribute", label: "Attribute" },
  { value: "value", label: "Function value", xpathOnly: true },
];

/** A name not used yet: "Extractor 3". */
function freshName(base: string, taken: string[]) {
  for (let n = taken.length + 1; ; n++) {
    const name = `${base} ${n}`;
    if (!taken.includes(name)) return name;
  }
}

/** Kinds that read the source or structured data have no mode to pick. */
const modeFor = (kind: ExtractorKind, mode: ExtractMode): ExtractMode =>
  kind === "regex" || kind === "json_ld" ? "value" : mode === "value" && kind === "css" ? "text" : mode;

function ExtractorRow({ e, i, issues, onChange, onRemove }: { e: Extractor; i: number; issues: Issue[]; onChange: (e: Extractor) => void; onRemove: () => void }) {
  const kind = KINDS.find((k) => k.value === e.kind) ?? KINDS[0];
  const hasMode = e.kind === "css" || e.kind === "xpath";
  const n = i + 1;
  return (
    <li className="flex flex-col gap-1.5 px-3.5 py-2.5" data-testid={`extractor-${i}`}>
      <div className="flex flex-wrap items-center gap-2">
        <Input aria-label={`Extractor ${n} name`} className="w-40" value={e.name} onChange={(ev) => onChange({ ...e, name: ev.target.value })} />
        <Select aria-label={`Extractor ${n} type`} className="w-44" value={e.kind} onChange={(ev) => {
          const k = ev.target.value as ExtractorKind;
          onChange({ ...e, kind: k, mode: modeFor(k, e.mode) });
        }}>
          {KINDS.map((k) => (
            <option key={k.value} value={k.value}>
              {k.label}
            </option>
          ))}
        </Select>
        <Button variant="quiet" icon size="sm" aria-label={`Remove extractor ${n}`} className="ml-auto" onClick={onRemove}>
          <X className="size-3.5" />
        </Button>
      </div>
      <Input aria-label={`Extractor ${n} expression`} mono placeholder={kind.placeholder} value={e.expr} onChange={(ev) => onChange({ ...e, expr: ev.target.value })} />
      {(hasMode || e.kind === "json_ld") && (
        <div className="flex flex-wrap items-center gap-2">
          {hasMode && (
            <Select aria-label={`Extractor ${n} returns`} className="w-40" value={e.mode} onChange={(ev) => onChange({ ...e, mode: ev.target.value as ExtractMode })}>
              {MODES.filter((m) => !m.xpathOnly || e.kind === "xpath").map((m) => (
                <option key={m.value} value={m.value}>
                  {m.label}
                </option>
              ))}
            </Select>
          )}
          {hasMode && e.mode === "attribute" && (
            <Input aria-label={`Extractor ${n} attribute`} mono placeholder="href" className="w-36" value={e.attr ?? ""} onChange={(ev) => onChange({ ...e, attr: ev.target.value })} />
          )}
          {e.kind === "json_ld" && !e.expr.trim().startsWith("$") && (
            <Input
              aria-label={`Extractor ${n} item type`}
              placeholder="Item type, for example Product"
              className="w-56"
              value={e.json_type ?? ""}
              onChange={(ev) => onChange({ ...e, json_type: ev.target.value || undefined })}
            />
          )}
        </div>
      )}
      <IssueList issues={issues} />
    </li>
  );
}

function SearchRow({ s, i, issues, onChange, onRemove }: { s: Search; i: number; issues: Issue[]; onChange: (s: Search) => void; onRemove: () => void }) {
  const n = i + 1;
  const limit: ElementKind | "none" = s.within?.kind ?? "none";
  return (
    <li className="flex flex-col gap-1.5 px-3.5 py-2.5" data-testid={`search-${i}`}>
      <div className="flex flex-wrap items-center gap-2">
        <Input aria-label={`Search ${n} name`} className="w-40" value={s.name} onChange={(ev) => onChange({ ...s, name: ev.target.value })} />
        <Select aria-label={`Search ${n} condition`} className="w-40" value={s.condition} onChange={(ev) => onChange({ ...s, condition: ev.target.value as SearchCondition })}>
          <option value="contains">Contains</option>
          <option value="not_contains">Does not contain</option>
        </Select>
        <Select aria-label={`Search ${n} looks in`} className="w-36" value={s.target} onChange={(ev) => onChange({ ...s, target: ev.target.value as SearchTarget })}>
          <option value="html">HTML</option>
          <option value="text">Visible text</option>
        </Select>
        <Button variant="quiet" icon size="sm" aria-label={`Remove search ${n}`} className="ml-auto" onClick={onRemove}>
          <X className="size-3.5" />
        </Button>
      </div>
      <Input aria-label={`Search ${n} term`} mono placeholder={s.regex ? "consent-v\\d+\\.js" : "consent-v2.js"} value={s.term} onChange={(ev) => onChange({ ...s, term: ev.target.value })} />
      <div className="flex flex-wrap items-center gap-3 text-sm text-fg-2">
        <span className="flex items-center gap-1.5">
          <Switch label={`Search ${n} regular expression`} checked={s.regex} onChange={(regex) => onChange({ ...s, regex })} />
          Regular expression
        </span>
        <span className="flex items-center gap-1.5">
          <Switch label={`Search ${n} ignore case`} checked={s.ignore_case} onChange={(ignore_case) => onChange({ ...s, ignore_case })} />
          Ignore case
        </span>
        <Select
          aria-label={`Search ${n} only inside`}
          className="w-44"
          value={limit}
          onChange={(ev) => {
            const k = ev.target.value as ElementKind | "none";
            onChange({ ...s, within: k === "none" ? undefined : { kind: k, expr: s.within?.expr ?? "" } });
          }}
        >
          <option value="none">Anywhere on the page</option>
          <option value="css">Inside CSS selector</option>
          <option value="xpath">Inside XPath</option>
        </Select>
        {s.within && (
          <Input
            aria-label={`Search ${n} element`}
            mono
            placeholder={s.within.kind === "css" ? "main" : "//main"}
            className="w-48"
            value={s.within.expr}
            onChange={(ev) => onChange({ ...s, within: { kind: s.within!.kind, expr: ev.target.value } })}
          />
        )}
      </div>
      <IssueList issues={issues} />
    </li>
  );
}

function RuleTester({ transport, setup, rules }: { transport: Transport; setup: CrawlSetup; rules: CustomRules }) {
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const [outcome, setOutcome] = useState<RuleTestOutcome | null>(null);
  const [error, setError] = useState<string | null>(null);
  const empty = rules.extractors.length + rules.searches.length === 0;

  const test = async () => {
    if (!setup.config || !url.trim()) return;
    setBusy(true);
    setError(null);
    try {
      setOutcome(await transport.customTestUrl(rules, url.trim(), setup.config));
    } catch (e) {
      setOutcome(null);
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex w-full flex-col gap-2">
      <div className="flex items-center gap-2">
        <Input
          aria-label="Page to test"
          placeholder="https://example.com/product"
          mono
          className="min-w-0 flex-1"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && void test()}
        />
        <Button size="sm" disabled={busy || empty || !url.trim()} onClick={() => void test()}>
          {busy ? "Testing…" : "Test rules"}
        </Button>
      </div>
      {error && (
        <Notice kind="bad" role="alert">
          {error}
        </Notice>
      )}
      {outcome && (
        <>
          <div className="text-sm text-fg-3" data-selectable>
            {outcome.url}
            {outcome.status !== null && ` · ${outcome.status}`} · {outcome.ms} ms
          </div>
          <table className="w-full border-collapse text-sm" aria-label="Test results">
            <thead>
              <tr className="text-left text-xs text-fg-3">
                <th className="py-1 pr-2 font-medium">Rule</th>
                <th className="py-1 pr-2 font-medium">Matches</th>
                <th className="py-1 font-medium">Found</th>
              </tr>
            </thead>
            <tbody>
              {outcome.results.map((r) => (
                <tr key={`${r.kind}:${r.name}`} className="border-t border-line align-top">
                  <td className="py-1 pr-2">{r.name}</td>
                  <td className="py-1 pr-2 tabular">{r.count.toLocaleString()}</td>
                  <td className="py-1">
                    <RuleFound kind={r.kind} flags={r.flags} values={r.values} error={r.error} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </div>
  );
}

const FLAG_LABELS: Record<string, { label: string; tone: "ok" | "warn" | "bad" | "neutral" }> = {
  no_match: { label: "No match", tone: "neutral" },
  more_than_stored: { label: "More than stored", tone: "warn" },
  truncated: { label: "Value cut", tone: "warn" },
  timed_out: { label: "Timed out", tone: "bad" },
  error: { label: "Error", tone: "bad" },
  element_not_found: { label: "Element not found", tone: "warn" },
  not_available: { label: "No stored HTML", tone: "neutral" },
  bad_json: { label: "Bad JSON-LD", tone: "warn" },
};

/** Values, or for a search whether the page meets it, with the flags. */
export function RuleFound({ kind, flags, values, error }: { kind: string; flags: string[]; values: string[]; error: string | null }) {
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <div className="flex flex-wrap gap-1">
        {kind === "search" && (flags.includes("meets") ? <Badge tone="ok">Meets</Badge> : <Badge tone="neutral">Does not meet</Badge>)}
        {flags
          .filter((f) => f in FLAG_LABELS && !(kind === "search" && f === "no_match"))
          .map((f) => (
            <Badge key={f} tone={FLAG_LABELS[f].tone}>
              {FLAG_LABELS[f].label}
            </Badge>
          ))}
      </div>
      {error && <span className="text-bad">{error}</span>}
      {values.length > 0 && (
        <ul className="m-0 flex list-none flex-col gap-0.5 p-0">
          {values.slice(0, 5).map((v, i) => (
            <li key={i} className="truncate font-code text-fg-2" title={v} data-selectable>
              {v}
            </li>
          ))}
          {values.length > 5 && <li className="text-fg-3">{(values.length - 5).toLocaleString()} more</li>}
        </ul>
      )}
    </div>
  );
}

function RunOnCrawl({ transport, rules, path, running }: { transport: Transport; rules: CustomRules; path: string; running: boolean }) {
  const run = useCustomRun(transport, path);
  const empty = rules.extractors.length + rules.searches.length === 0;
  const why = running ? "Pause or stop the crawl first." : empty ? "Add a rule first." : null;
  return (
    <div className="flex w-full flex-col gap-2">
      <div className="flex items-center gap-2">
        <Button size="sm" disabled={why !== null || run.running} onClick={() => void run.start(rules)}>
          <Play aria-hidden className="size-3.5" /> Run on the open crawl
        </Button>
        {run.running && run.progress && (
          <>
            <span className="text-sm text-fg-2 tabular" role="status">
              {run.progress.done.toLocaleString()} of {run.progress.total.toLocaleString()} pages
            </span>
            <Bar value={run.progress.total ? run.progress.done / run.progress.total : 0} tone="accent" label="Rule run progress" className="w-32" />
            <Button size="sm" variant="quiet" onClick={run.cancel}>
              Stop
            </Button>
          </>
        )}
        {why && !run.running && <span className="text-sm text-fg-3">{why}</span>}
      </div>
      {run.result && (
        <Notice kind={run.result.state === "failed" ? "bad" : run.result.state === "partial" ? "warn" : "info"} role="status">
          {runOutcomeText(run.result)}
        </Notice>
      )}
    </div>
  );
}

export function CustomRulesPanel({
  setup,
  transport,
  crawlPath,
  crawlRunning,
}: {
  setup: CrawlSetup;
  transport: Transport;
  crawlPath: string | null;
  crawlRunning: boolean;
}) {
  const { config, set, issues } = setup;
  const [fileNote, setFileNote] = useState<{ kind: "info" | "bad"; text: string } | null>(null);
  if (!config) return null;
  const rules = config.custom;
  const setRules = (f: (r: CustomRules) => CustomRules) => set((c) => ({ ...c, custom: f(c.custom) }));
  const names = (k: "extractors" | "searches") => rules[k].map((r) => r.name);

  const addExtractor = () =>
    setRules((r) => ({ ...r, extractors: [...r.extractors, { name: freshName("Extractor", names("extractors")), kind: "css", expr: "", mode: "text" }] }));
  const addSearch = () =>
    setRules((r) => ({
      ...r,
      searches: [...r.searches, { name: freshName("Search", names("searches")), term: "", regex: false, condition: "contains", target: "html", ignore_case: false }],
    }));

  const importFile = async () => {
    const path = await transport.pickRulesFile();
    if (!path) return;
    try {
      const more = await transport.customImport(path);
      setRules((r) => ({ extractors: [...r.extractors, ...more.extractors], searches: [...r.searches, ...more.searches] }));
      const n = more.extractors.length + more.searches.length;
      setFileNote({ kind: "info", text: `Added ${n} ${n === 1 ? "rule" : "rules"} from ${path}.` });
    } catch (e) {
      setFileNote({ kind: "bad", text: String(e) });
    }
  };
  const exportFile = async () => {
    const path = await transport.pickRulesSavePath("custom-rules");
    if (!path) return;
    try {
      await transport.customExport(path, rules);
      setFileNote({ kind: "info", text: `Saved the rules to ${path}.` });
    } catch (e) {
      setFileNote({ kind: "bad", text: String(e) });
    }
  };

  return (
    <>
      {crawlRunning && <Notice kind="info">Changes apply to the next crawl. Once this crawl stops, the rules can also run on it.</Notice>}
      <IssueList issues={issues.filter((i) => i.path === "custom.extractors" || i.path === "custom.searches")} />
      <Group title="Extraction">
        <ul className="m-0 flex list-none flex-col divide-y divide-line p-0" aria-label="Extractors">
          {rules.extractors.map((e, i) => (
            <ExtractorRow
              key={i}
              e={e}
              i={i}
              issues={issuesAt(issues, `custom.extractors[${i}]`)}
              onChange={(x) => setRules((r) => ({ ...r, extractors: r.extractors.map((y, j) => (j === i ? x : y)) }))}
              onRemove={() => setRules((r) => ({ ...r, extractors: r.extractors.filter((_, j) => j !== i) }))}
            />
          ))}
          <li className="px-3.5 py-2.5">
            <Button size="sm" onClick={addExtractor}>
              <Plus className="size-3.5" /> Add extractor
            </Button>
            {rules.extractors.length === 0 && <span className="ml-2 text-sm text-fg-3">Pull a value from every page into its own column.</span>}
          </li>
        </ul>
      </Group>
      <Group title="Search">
        <ul className="m-0 flex list-none flex-col divide-y divide-line p-0" aria-label="Searches">
          {rules.searches.map((s, i) => (
            <SearchRow
              key={i}
              s={s}
              i={i}
              issues={issuesAt(issues, `custom.searches[${i}]`)}
              onChange={(x) => setRules((r) => ({ ...r, searches: r.searches.map((y, j) => (j === i ? x : y)) }))}
              onRemove={() => setRules((r) => ({ ...r, searches: r.searches.filter((_, j) => j !== i) }))}
            />
          ))}
          <li className="px-3.5 py-2.5">
            <Button size="sm" onClick={addSearch}>
              <Plus className="size-3.5" /> Add search
            </Button>
            {rules.searches.length === 0 && <span className="ml-2 text-sm text-fg-3">Find pages that do or do not contain something.</span>}
          </li>
        </ul>
      </Group>
      <Group title="Try the rules">
        <Row label="Test on one page" stacked help="Fetches the page once, as this profile would, and shows what each rule finds.">
          {() => <RuleTester transport={transport} setup={setup} rules={rules} />}
        </Row>
        {crawlPath && (
          <Row label="Run on the open crawl" stacked help="Uses the HTML stored with the crawl; no page is requested again. Rules with the same name replace their earlier values.">
            {() => <RunOnCrawl transport={transport} rules={rules} path={crawlPath} running={crawlRunning} />}
          </Row>
        )}
      </Group>
      <Group title="Rule files">
        <Row label="Share rules" help="Rule files hold only the rules, so they can be added to any profile.">
          {() => (
            <div className="flex items-center gap-2">
              <Button size="sm" onClick={() => void importFile()}>
                <FileUp aria-hidden className="size-3.5" /> Import…
              </Button>
              <Button size="sm" disabled={rules.extractors.length + rules.searches.length === 0} onClick={() => void exportFile()}>
                <FileDown aria-hidden className="size-3.5" /> Export…
              </Button>
            </div>
          )}
        </Row>
        {fileNote && (
          <div className="px-3.5 py-2.5">
            <Notice kind={fileNote.kind} role={fileNote.kind === "bad" ? "alert" : "status"}>
              {fileNote.text}
            </Notice>
          </div>
        )}
      </Group>
    </>
  );
}
