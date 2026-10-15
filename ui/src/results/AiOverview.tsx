// The AI search tab's site overview (spec 015 US5, FR-039..FR-041): the site
// score with its parts, the fixes worth most, crawler access per host and
// llms.txt. The table below it lists the pages.

import { useState } from "react";
import { ChevronDown, ChevronRight, Play, Sparkles } from "lucide-react";
import type { AeoRule } from "../generated/AeoRule";
import type { AiHostSummary } from "../generated/AiHostSummary";
import type { AiProgress } from "../generated/AiProgress";
import type { Part } from "../generated/Part";
import type { Transport } from "../transport";
import { Badge, Bar, Button, cn, Notice, type Tone } from "../components/ui";
import { toneText } from "./format";
import { useAeo, useAeoRules } from "./useAeo";

export const partLabels: Record<Part, string> = {
  access: "Access",
  readable: "Readable without JS",
  answer: "Answers",
  trust: "Trust",
  structured: "Structured data",
  site: "Site checks",
};

export const scoreTone = (score: number): Tone => (score >= 80 ? "ok" : score >= 50 ? "warn" : "bad");

/** Points are stored in hundredths. */
export const points = (hundredths: number) => {
  const p = hundredths / 100;
  return Number.isInteger(p) ? String(p) : p.toFixed(1);
};

export const ruleName = (rules: Map<string, AeoRule>, id: string) => rules.get(id)?.rule.name ?? id;

const llmsLabels: Record<string, [string, Tone]> = {
  ok: ["Found", "ok"],
  not_found: ["Not found", "warn"],
  error: ["Could not be read", "bad"],
  too_large: ["Over 1 MB", "warn"],
  skipped: ["Not fetched (robots.txt)", "neutral"],
};

const verdictTone: Record<string, Tone> = { allowed: "ok", blocked: "bad", unavailable: "warn", unknown: "neutral" };

const textTone = (t: Tone) => (t === "neutral" ? "text-fg-2" : toneText[t]);

function progressText(p: AiProgress | null) {
  if (!p) return "Starting the AI checks…";
  if (p.pass === "hosts") return `Reading robots.txt rules, host ${p.done.toLocaleString()} of ${p.total.toLocaleString()}`;
  if (p.pass === "pages") return `Checking pages, ${p.done.toLocaleString()} of ${p.total.toLocaleString()}`;
  return "Checking the site";
}

const progressValue = (p: AiProgress | null) => (p && p.pass !== "site" ? (p.total ? p.done / p.total : 0) : p ? 1 : 0);

function Hosts({ hosts }: { hosts: AiHostSummary[] }) {
  return (
    <div className="flex flex-col gap-3">
      {hosts.map((h) => {
        const llms = h.llms_txt ? (llmsLabels[h.llms_txt.outcome] ?? [h.llms_txt.outcome, "neutral"]) : null;
        return (
          <section key={h.host_key} aria-label={`Crawler access for ${h.host_key}`} className="flex flex-col gap-1">
            <div className="flex items-center gap-2 text-sm">
              <h3 className="m-0 truncate text-sm font-medium">{h.host_key}</h3>
              {h.robots !== "ok" && <Badge tone="warn">robots.txt {h.robots.replace(/_/g, " ")}</Badge>}
              {llms && (
                <span className="ml-auto text-fg-3">
                  llms.txt <Badge tone={llms[1]}>{llms[0]}</Badge>
                </span>
              )}
            </div>
            <table className="w-full text-sm tabular">
              <thead className="text-left text-xs text-fg-3">
                <tr>
                  <th className="font-normal">Crawler</th>
                  <th className="font-normal">Home page</th>
                  <th className="text-right font-normal">URLs blocked</th>
                  <th className="pl-3 font-normal">Deciding rule</th>
                </tr>
              </thead>
              <tbody>
                {h.access.map((a) => (
                  <tr key={a.bot}>
                    <td>{a.matched_group ?? a.bot}</td>
                    <td>
                      <span className={textTone(verdictTone[a.home_verdict] ?? "neutral")}>{a.home_verdict}</span>
                    </td>
                    <td className="text-right">
                      {a.urls_blocked.toLocaleString()} of {a.urls_total.toLocaleString()}
                    </td>
                    <td className="truncate pl-3 font-mono text-xs text-fg-2">{a.rules[0] ? `line ${a.rules[0].line_no}: ${a.rules[0].rule}` : "–"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {h.signals.length > 0 && (
              <ul className="m-0 list-none p-0 text-sm text-fg-2">
                {h.signals.map((s) => (
                  <li key={s.line_no}>
                    <span className="font-mono text-xs">{s.key}</span> {s.meaning}
                  </li>
                ))}
              </ul>
            )}
          </section>
        );
      })}
    </div>
  );
}

export function AiOverview({ transport, path, live }: { transport: Transport; path: string; live: boolean }) {
  const aeo = useAeo(transport, path);
  const rules = useAeoRules(transport);
  const [open, setOpen] = useState(false);
  const s = aeo.summary;

  if (!aeo.loaded) return null;

  if (aeo.running) {
    return (
      <div role="status" aria-label="AI checks" className="flex items-center gap-3 text-sm">
        <span className="text-fg-2">{progressText(aeo.running.progress)}</span>
        <Bar value={progressValue(aeo.running.progress)} tone="accent" label="AI checks progress" className="w-40" />
        <Button size="sm" variant="quiet" onClick={aeo.cancel} disabled={aeo.running.runId === null}>
          Cancel
        </Button>
      </div>
    );
  }

  const runButton = (label: string) => (
    <Button size="sm" variant={s ? "quiet" : "primary"} onClick={() => void aeo.run()} disabled={live}>
      <Play aria-hidden className="size-3.5" />
      {label}
    </Button>
  );

  if (!s) {
    return (
      <div aria-label="AI checks" className="flex items-center gap-3 text-sm">
        <Sparkles aria-hidden className="size-4 text-fg-3" />
        <span className="text-fg-2">{live ? "The AI checks run when the crawl completes." : "AI checks have not run on this crawl."}</span>
        {aeo.error && <Notice kind="bad">{aeo.error}</Notice>}
        {aeo.cancelled && <span className="text-fg-3">Cancelled.</span>}
        <div className="ml-auto">{runButton("Run AI checks")}</div>
      </div>
    );
  }

  const tone = scoreTone(s.score);
  return (
    <section aria-label="AI checks" className="flex flex-col gap-2">
      <div className="flex items-center gap-4 text-sm">
        <div className="flex items-baseline gap-1.5">
          <span className={cn("text-xl font-semibold tabular", textTone(tone))} data-testid="ai-site-score">
            {s.score}
          </span>
          <span className="text-fg-3">/ 100 AI search score</span>
        </div>
        <ul aria-label="Score parts" className="m-0 flex list-none flex-wrap items-center gap-3 p-0">
          {s.parts.map((p) => (
            <li key={p.part} className="flex items-center gap-1.5">
              <span className="text-fg-3">{partLabels[p.part]}</span>
              {p.score === null ? (
                <span className="text-fg-3" title="No page had data for this part">
                  n/a
                </span>
              ) : (
                <>
                  <Bar value={p.max ? p.score / p.max : 0} tone={scoreTone((p.score / Math.max(p.max, 1)) * 100)} label={partLabels[p.part]} className="w-10" />
                  <span className="tabular">
                    {p.score}/{p.max}
                  </span>
                </>
              )}
            </li>
          ))}
          <li className="flex items-center gap-1.5">
            <span className="text-fg-3">{partLabels.site}</span>
            <span className="tabular">{s.site_checks}/100</span>
          </li>
        </ul>
        <div className="ml-auto flex items-center gap-1.5">
          {aeo.error && <Notice kind="bad">{aeo.error}</Notice>}
          <Button size="sm" variant="quiet" aria-expanded={open} onClick={() => setOpen(!open)}>
            {open ? <ChevronDown aria-hidden className="size-3.5" /> : <ChevronRight aria-hidden className="size-3.5" />}
            Details
          </Button>
          {runButton("Run again")}
        </div>
      </div>
      {s.provisional_pages > 0 && (
        <Notice kind="info">
          {s.provisional_pages.toLocaleString()} of {s.pages_analyzed.toLocaleString()} pages have provisional scores: JavaScript rendering did not run on them.
        </Notice>
      )}
      {open && (
        <div className="grid max-h-80 grid-cols-2 gap-4 overflow-y-auto border-t border-line pt-2">
          <section aria-label="Top fixes" className="flex flex-col gap-1">
            <h3 className="m-0 text-xs font-semibold text-fg-3">Top fixes</h3>
            {s.top_fixes.length === 0 ? (
              <p className="m-0 text-sm text-fg-3">Nothing to fix: every check passed.</p>
            ) : (
              <ol className="m-0 flex list-none flex-col gap-1.5 p-0 text-sm">
                {s.top_fixes.map((f) => (
                  <li key={f.rule_id} className="flex flex-col">
                    <div className="flex items-baseline gap-2">
                      <span className="font-medium">{ruleName(rules, f.rule_id)}</span>
                      <span className="ml-auto shrink-0 text-ok tabular">+{points(f.gain)} points</span>
                    </div>
                    <span className="text-fg-3">
                      {f.pages > 0 ? `${f.pages.toLocaleString()} ${f.pages === 1 ? "page" : "pages"}. ` : "Site check. "}
                      {rules.get(f.rule_id)?.rule.how_to_fix}
                    </span>
                  </li>
                ))}
              </ol>
            )}
            <h3 className="m-0 mt-2 text-xs font-semibold text-fg-3">Who the site is</h3>
            <dl className="m-0 grid grid-cols-3 gap-x-3 gap-y-0.5 text-sm">
              <dt className="text-fg-3">Entity</dt>
              <dd className="col-span-2 m-0">{s.entity ? `${s.entity.name ?? "Unnamed"} (${s.entity.kind})` : "No Organization or Person markup"}</dd>
              <dt className="text-fg-3">About page</dt>
              <dd className="col-span-2 m-0 break-all">{s.about_url ?? "Not found"}</dd>
              <dt className="text-fg-3">Contact page</dt>
              <dd className="col-span-2 m-0 break-all">{s.contact_url ?? "Not found"}</dd>
            </dl>
          </section>
          <Hosts hosts={s.hosts} />
        </div>
      )}
    </section>
  );
}
