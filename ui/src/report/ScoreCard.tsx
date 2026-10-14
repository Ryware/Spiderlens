import { Badge, Bar } from "../components/ui";
import type { Report } from "../generated/Report";
import type { Target } from "../generated/Target";
import { PRIORITY_LABEL, PRIORITY_TONE, points, scoreBand } from "./format";

/** Health score, where its points went, and the costliest issues (FR-008 to FR-012). */
export function ScoreCard({ report, onOpen }: { report: Report; onOpen: (t: Target) => void }) {
  const s = report.score;
  const band = s.value === null ? null : scoreBand(s.value);
  const lost = s.by_priority.filter((p) => p.pages > 0);
  return (
    <section aria-label="Health score" className="flex gap-6 rounded-lg border border-line bg-surface p-5">
      <div className="flex flex-col items-center justify-center gap-1 px-4">
        <div data-testid="score" className="text-xl leading-none font-semibold tabular">
          {s.value ?? "–"}
        </div>
        {band ? <Badge tone={band.tone}>{band.label}</Badge> : <span className="text-sm text-fg-3">{s.unavailable ?? "Nothing to score"}</span>}
        {report.provisional && <Badge tone="info">Provisional</Badge>}
      </div>
      <div className="flex min-w-0 flex-col gap-3">
        <p className="m-0 text-sm text-fg-2">
          Out of 100, over {s.pages.toLocaleString()} internal HTML pages. Each page counts its worst issue.
          {s.partial && " Some checks could not run on this crawl, so the score leaves them out."}
        </p>
        {lost.length > 0 && (
          <table className="w-full text-sm">
            <caption className="sr-only">Points lost by priority</caption>
            <tbody>
              {lost.map((p) => (
                <tr key={p.priority}>
                  <td className="py-0.5 pr-3">
                    <Badge tone={PRIORITY_TONE[p.priority]}>{PRIORITY_LABEL[p.priority]}</Badge>
                  </td>
                  <td className="py-0.5 pr-3 text-fg-2 tabular">{p.pages.toLocaleString()} pages</td>
                  <td className="py-0.5 text-right tabular">−{points(p.points)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {s.issues.length > 0 && (
          <div className="flex flex-col gap-1">
            <h3 className="m-0 text-sm font-semibold">Fix first</h3>
            <ul className="m-0 flex list-none flex-col gap-1 p-0">
              {s.issues.slice(0, 5).map((i) => (
                <li key={i.id} className="flex items-center gap-2 text-sm">
                  <button type="button" className="min-w-0 flex-1 truncate text-left text-accent-text hover:underline" onClick={() => onOpen({ kind: "issue", id: i.id })}>
                    {i.name}
                  </button>
                  <span className="text-fg-3 tabular">{i.pages.toLocaleString()} pages</span>
                  <Bar value={s.issues[0].share ? i.share / s.issues[0].share : 0} tone={PRIORITY_TONE[i.priority]} label={`${i.name} share of points lost`} />
                  <span className="w-14 text-right tabular" title="Points the score gains when this is fixed">
                    {i.gain > 0 ? `+${points(i.gain)}` : "–"}
                  </span>
                </li>
              ))}
            </ul>
          </div>
        )}
        {s.not_run.length > 0 && <p className="m-0 text-xs text-fg-3">Not checked on this crawl: {s.not_run.join(", ")}.</p>}
      </div>
    </section>
  );
}
