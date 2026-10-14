// The selected page's AI checks in the details pane (spec 015 FR-043): its
// score, type, dates and author, and each check it lost points on.

import { useEffect, useState, type ReactNode } from "react";
import type { AiPageResult } from "../generated/AiPageResult";
import type { Transport } from "../transport";
import { Badge, cn } from "../components/ui";
import { points, ruleName, scoreTone } from "./AiOverview";
import { toneText } from "./format";
import { useAeoRules } from "./useAeo";

export function AiPageSection({ transport, path, id, section }: { transport: Transport; path: string; id: number; section: (title: string, body: ReactNode) => ReactNode }) {
  const rules = useAeoRules(transport);
  const [page, setPage] = useState<AiPageResult | null | undefined>(undefined);
  useEffect(() => {
    let off = false;
    setPage(undefined);
    transport.aeoPage(path, id).then(
      (p) => !off && setPage(p),
      () => !off && setPage(null),
    );
    return () => {
      off = true;
    };
  }, [transport, path, id]);

  if (page === undefined) return null;
  if (page === null) return section("AI search", <p className="m-0 text-sm text-fg-3">Not checked: the AI checks look at crawled HTML pages only.</p>);

  const tone = page.score === null ? null : scoreTone(page.score);
  const failed = page.checks.filter((c) => c.outcome === "fail");
  const notAvailable = page.checks.filter((c) => c.outcome === "not_available");
  return section(
    "AI search",
    <div className="flex flex-col gap-2" aria-label="AI search checks">
      <div className="flex items-center gap-2 text-sm">
        <span className={cn("text-md font-semibold tabular", tone && tone !== "neutral" ? toneText[tone] : "text-fg-2")} data-testid="ai-page-score">
          {page.score ?? "–"}
        </span>
        <span className="text-fg-3">/ 100</span>
        {page.provisional && <Badge tone="info">Provisional</Badge>}
        <span className="ml-auto text-fg-2" title={page.page_type_reason ?? undefined}>
          {page.page_type[0].toUpperCase() + page.page_type.slice(1)} page
        </span>
      </div>
      <dl className="m-0 grid grid-cols-3 gap-x-3 gap-y-1 text-sm" data-selectable>
        <dt className="text-fg-3">Published</dt>
        <dd className="col-span-2 m-0">{page.published ?? "Not found"}</dd>
        <dt className="text-fg-3">Updated</dt>
        <dd className="col-span-2 m-0">{page.modified ?? "Not found"}</dd>
        <dt className="text-fg-3">Author</dt>
        <dd className="col-span-2 m-0">{page.author ?? "Not found"}</dd>
        <dt className="text-fg-3">Blocked for</dt>
        <dd className={cn("col-span-2 m-0", page.blocked_crawlers.length > 0 && "text-bad")}>{page.blocked_crawlers.length ? page.blocked_crawlers.join(", ") : "No AI crawler"}</dd>
      </dl>
      {failed.length > 0 ? (
        <ul aria-label="Failed AI checks" className="m-0 flex list-none flex-col gap-1.5 p-0 text-sm">
          {failed.map((c) => (
            <li key={c.rule_id} className="flex flex-col">
              <div className="flex items-baseline gap-2">
                <span className="font-medium">{ruleName(rules, c.rule_id)}</span>
                {c.points_lost > 0 && <span className="ml-auto shrink-0 text-bad tabular">−{points(c.points_lost)}</span>}
              </div>
              {c.evidence && (
                <span className="break-words text-fg-2" data-selectable>
                  {c.evidence}
                </span>
              )}
              {rules.get(c.rule_id) && <span className="text-fg-3">{rules.get(c.rule_id)!.rule.how_to_fix}</span>}
            </li>
          ))}
        </ul>
      ) : (
        <p className="m-0 text-sm text-ok">Every check that applies passed.</p>
      )}
      {notAvailable.length > 0 && (
        <p className="m-0 text-sm text-fg-3">
          Not checked for lack of data: {notAvailable.map((c) => ruleName(rules, c.rule_id)).join(", ")}.
        </p>
      )}
    </div>,
  );
}
