// The Agents tab's site summary (spec 016 FR-018): the crawl's agent score
// with its four parts, then (under Details) the rules that cost the most
// points, the checks that could not run and what each host offers agents.
// The table below it lists the pages.

import { useEffect, useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import type { AgentPart } from "../generated/AgentPart";
import type { AgentsSummary } from "../generated/AgentsSummary";
import type { EntryPointRow } from "../generated/EntryPointRow";
import type { Transport } from "../transport";
import { Badge, Bar, Button, cn, type Tone } from "../components/ui";
import { toneText } from "./format";

export const agentPartLabels: Record<AgentPart, string> = {
  access: "Access",
  discovery: "Discovery",
  reading: "Reading",
  acting: "Acting",
};

const tone = (score: number): Tone => (score >= 80 ? "ok" : score >= 50 ? "warn" : "bad");
const textTone = (t: Tone) => (t === "neutral" ? "text-fg-2" : toneText[t]);
const num = (n: number) => (Number.isInteger(n) ? String(n) : n.toFixed(1));

const kindLabels: Record<string, string> = {
  ai_catalog: "AI Catalog",
  api_catalog: "API catalog",
  a2a_card: "Agent card",
  service_desc: "Service description",
  service_doc: "Service documentation",
};

const outcomeLabels: Record<string, [string, Tone]> = {
  ok: ["Found", "ok"],
  absent: ["Absent", "neutral"],
  invalid: ["Invalid", "bad"],
  error: ["Could not be read", "bad"],
  blocked_robots: ["Blocked by robots.txt", "warn"],
  too_large: ["Over 1 MB", "warn"],
  wrong_type: ["Wrong content type", "warn"],
  not_fetched: ["Not fetched", "neutral"],
};

function EntryPoints({ rows }: { rows: EntryPointRow[] }) {
  const hosts = new Map<string, EntryPointRow[]>();
  for (const r of rows) hosts.set(r.host_key, [...(hosts.get(r.host_key) ?? []), r]);
  if (hosts.size === 0) return <p className="m-0 text-sm text-fg-3">No entry points were recorded for this crawl.</p>;
  return (
    <div className="flex flex-col gap-3">
      {[...hosts].map(([host, list]) => (
        <section key={host} aria-label={`Entry points for ${host}`} className="flex flex-col gap-1">
          <h3 className="m-0 truncate text-sm font-medium">{host}</h3>
          <ul className="m-0 flex list-none flex-col gap-1 p-0 text-sm">
            {list.map((r) => {
              const [label, t] = outcomeLabels[r.outcome] ?? [r.outcome, "neutral" as Tone];
              return (
                <li key={r.id} className="flex flex-col">
                  <div className="flex items-center gap-2">
                    <span>{kindLabels[r.kind] ?? r.kind}</span>
                    <Badge tone={t}>{label}</Badge>
                    <span className="ml-auto truncate font-mono text-xs text-fg-3">{r.url}</span>
                  </div>
                  {r.problems.map((p) => (
                    <span key={p.code} className="text-fg-2">
                      {p.message}
                    </span>
                  ))}
                </li>
              );
            })}
          </ul>
        </section>
      ))}
    </div>
  );
}

export function AgentsOverview({ transport, path }: { transport: Transport; path: string }) {
  const [summary, setSummary] = useState<AgentsSummary | null>(null);
  const [entries, setEntries] = useState<EntryPointRow[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    let off = false;
    setSummary(null);
    setEntries([]);
    setError(null);
    transport.agentsSummary(path).then(
      (s) => !off && setSummary(s),
      (e) => !off && setError(String(e)),
    );
    transport.agentsEntryPoints(path).then(
      (r) => !off && setEntries(r),
      () => {},
    );
    return () => {
      off = true;
    };
  }, [transport, path]);

  if (error) {
    return (
      <section aria-label="AI agents" className="text-sm text-bad">
        {error}
      </section>
    );
  }
  if (!summary) return null;

  if (!summary.available || summary.score === null) {
    return (
      <section aria-label="AI agents" className="flex items-center gap-2 text-sm">
        <span className="text-fg-2">AI agent score not available.</span>
        <span className="text-fg-3">{summary.unavailable_reason}</span>
      </section>
    );
  }

  const t = tone(summary.score);
  return (
    <section aria-label="AI agents" className="flex flex-col gap-2">
      <div className="flex items-center gap-4 text-sm">
        <div className="flex items-baseline gap-1.5">
          <span className={cn("text-xl font-semibold tabular", textTone(t))} data-testid="agents-score">
            {num(summary.score)}
          </span>
          <span className="text-fg-3">/ 100 AI agent score</span>
        </div>
        <ul aria-label="Agent score parts" className="m-0 flex list-none flex-wrap items-center gap-3 p-0">
          {summary.parts.map((p) => (
            <li key={p.part} className="flex items-center gap-1.5">
              <span className="text-fg-3">{agentPartLabels[p.part]}</span>
              {p.score === null ? (
                <span className="text-fg-3" title="No check of this part could run">
                  n/a
                </span>
              ) : (
                <>
                  <Bar value={p.score / 100} tone={tone(p.score)} label={agentPartLabels[p.part]} className="w-10" />
                  <span className="tabular">{num(p.score)}</span>
                </>
              )}
            </li>
          ))}
        </ul>
        <Button size="sm" variant="quiet" className="ml-auto" aria-expanded={open} onClick={() => setOpen(!open)}>
          {open ? <ChevronDown aria-hidden className="size-3.5" /> : <ChevronRight aria-hidden className="size-3.5" />}
          Details
        </Button>
      </div>
      {open && (
        <div className="grid max-h-80 grid-cols-2 gap-4 overflow-y-auto border-t border-line pt-2">
          <div className="flex flex-col gap-2">
            <section aria-label="Top rules by points lost" className="flex flex-col gap-1">
              <h3 className="m-0 text-xs font-semibold text-fg-3">Costing the most points</h3>
              {summary.top_failed.length === 0 ? (
                <p className="m-0 text-sm text-fg-3">Nothing to fix: every check passed.</p>
              ) : (
                <ol className="m-0 flex list-none flex-col gap-1 p-0 text-sm">
                  {summary.top_failed.map((f) => (
                    <li key={f.id} className="flex items-baseline gap-2">
                      <span className="font-medium">{f.name}</span>
                      <span className="text-fg-3">{f.site_wide ? "Site-wide" : `${(f.pages ?? 0).toLocaleString()} ${f.pages === 1 ? "page" : "pages"}`}</span>
                      <span className="ml-auto shrink-0 text-bad tabular">-{num(f.points_lost)} points</span>
                    </li>
                  ))}
                </ol>
              )}
            </section>
            {summary.not_checked.length > 0 && (
              <section aria-label="Checks that could not run" className="flex flex-col gap-1">
                <h3 className="m-0 text-xs font-semibold text-fg-3">Not checked</h3>
                <ul className="m-0 flex list-none flex-col gap-1 p-0 text-sm">
                  {summary.not_checked.map((n) => (
                    <li key={n.id} className="flex flex-col">
                      <span className="font-medium">{n.name}</span>
                      <span className="text-fg-3">{n.reason}</span>
                    </li>
                  ))}
                </ul>
              </section>
            )}
          </div>
          <EntryPoints rows={entries} />
        </div>
      )}
    </section>
  );
}
