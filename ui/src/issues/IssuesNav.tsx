import { useState } from "react";
import { ChevronRight, CircleAlert, CircleDot, Lightbulb, TriangleAlert, type LucideIcon } from "lucide-react";
import { cn, Tooltip } from "../components/ui";
import type { IssueCount } from "../generated/IssueCount";
import type { IssuesSummary } from "../generated/IssuesSummary";
import type { IssuePriority as Priority } from "../generated/IssuePriority";
import { issueTab } from "./useIssues";
import { PRIORITIES, priorityLabel, priorityTone } from "./priority";

const icons: Record<Priority, LucideIcon> = { high: CircleAlert, medium: TriangleAlert, low: CircleDot, opportunity: Lightbulb };
const toneText = { bad: "text-bad", warn: "text-warn", info: "text-info", ok: "text-ok", neutral: "text-fg-3" } as const;

const shownCount = (i: IssueCount) => (i.enabled && i.count ? i.count : 0);

/**
 * The sidebar's Issues group (spec 009 US1, D10): one row per priority with
 * its total, expanding into that priority's issues by count. Selecting an
 * issue opens its URLs as a results tab. "Show all rules" adds rules with
 * no URLs, that cannot run on this file, or that are turned off (FR-020).
 */
export function IssuesNav({
  summary,
  counting = false,
  active,
  collapsed,
  onSelect,
}: {
  summary: IssuesSummary | null;
  counting?: boolean;
  active: string;
  collapsed: boolean;
  onSelect: (tab: string) => void;
}) {
  const [open, setOpen] = useState<Record<Priority, boolean>>({ high: true, medium: false, low: false, opportunity: false });
  const [all, setAll] = useState(false);

  if (!summary) {
    return collapsed ? null : <p className="m-0 px-2 py-1 text-sm text-fg-3">Counting issues…</p>;
  }
  const group = (p: Priority) =>
    summary.issues
      .filter((i) => i.priority === p && (all || shownCount(i) > 0))
      .sort((a, b) => shownCount(b) - shownCount(a) || a.name.localeCompare(b.name));
  const total = (p: Priority) => summary.by_priority.find((t) => t.priority === p);

  if (collapsed) {
    return (
      <ul className="m-0 flex list-none flex-col gap-px p-0">
        {PRIORITIES.map((p) => {
          const Icon = icons[p];
          const first = group(p)[0];
          const t = total(p);
          const label = `${priorityLabel[p]}: ${t?.issues ?? 0} issues`;
          return (
            <li key={p}>
              <Tooltip content={label}>
                <button
                  type="button"
                  aria-label={label}
                  aria-disabled={!first || undefined}
                  onClick={() => first && onSelect(issueTab(first.id))}
                  className="flex h-7 w-full items-center justify-center rounded-md text-fg-2 hover:bg-surface-2"
                >
                  <Icon aria-hidden className={cn("size-4", toneText[priorityTone[p]])} />
                </button>
              </Tooltip>
            </li>
          );
        })}
      </ul>
    );
  }

  const provisional = summary.issues.some((i) => i.provisional && shownCount(i) > 0);
  return (
    <div>
      <ul aria-label="Issues" className="m-0 flex list-none flex-col gap-px p-0">
        {PRIORITIES.map((p) => {
          const Icon = icons[p];
          const items = group(p);
          const t = total(p);
          const expanded = open[p];
          return (
            <li key={p}>
              <button
                type="button"
                aria-expanded={expanded}
                onClick={() => setOpen((o) => ({ ...o, [p]: !o[p] }))}
                className="flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-base text-fg-2 hover:bg-surface-2"
              >
                <ChevronRight aria-hidden className={cn("size-3 shrink-0 text-fg-3 transition-transform duration-(--dur-fast)", expanded && "rotate-90")} />
                <Icon aria-hidden className={cn("size-4 shrink-0", toneText[priorityTone[p]])} />
                <span className="min-w-0 flex-1 truncate">{priorityLabel[p]}</span>
                <span className="text-xs text-fg-3 tabular" aria-label={`${t?.issues ?? 0} issues`}>
                  {t?.issues ?? 0}
                </span>
              </button>
              {expanded && (
                <ul aria-label={`${priorityLabel[p]} issues`} className="m-0 flex list-none flex-col gap-px p-0 pl-5">
                  {items.length === 0 && <li className="px-2 py-1 text-sm text-fg-3">No issues</li>}
                  {items.map((i) => {
                    const tab = issueTab(i.id);
                    const on = tab === active;
                    const count = !i.enabled ? "Off" : i.unavailable ? "n/a" : (i.count ?? 0).toLocaleString();
                    return (
                      <li key={i.id}>
                        <button
                          type="button"
                          aria-current={on ? "page" : undefined}
                          aria-describedby={i.unavailable ? `why-${i.id}` : undefined}
                          title={i.unavailable ?? undefined}
                          onClick={() => onSelect(tab)}
                          className={cn(
                            "flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-sm",
                            on ? "bg-selection text-fg" : shownCount(i) > 0 ? "text-fg-2 hover:bg-surface-2" : "text-fg-3 hover:bg-surface-2",
                          )}
                        >
                          <span className="min-w-0 flex-1 truncate">{i.name}</span>
                          <span className="text-xs tabular">
                            {count}
                            {i.provisional && shownCount(i) > 0 ? "*" : ""}
                          </span>
                        </button>
                        {/* Why it cannot run on this file, for every reader (US3, FR-020). */}
                        {i.unavailable && (
                          <p id={`why-${i.id}`} className="m-0 px-2 pb-1 text-2xs text-fg-3">
                            {i.unavailable}
                          </p>
                        )}
                      </li>
                    );
                  })}
                </ul>
              )}
            </li>
          );
        })}
      </ul>
      <div className="flex items-center justify-between px-2 pt-1">
        <button type="button" onClick={() => setAll((a) => !a)} aria-pressed={all} className="text-xs text-fg-3 hover:text-fg-2">
          {all ? "Hide rules without issues" : "Show all rules"}
        </button>
        {counting && <span className="text-2xs text-fg-3">* counting…</span>}
        {provisional && !counting && <span className="text-2xs text-fg-3">* until the crawl ends</span>}
        {/* FR-013: which rule set these issues come from. */}
        {!provisional && <span className="text-2xs text-fg-3">Rule set v{summary.rules_version}</span>}
      </div>
    </div>
  );
}
