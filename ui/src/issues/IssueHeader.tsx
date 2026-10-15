import { useState } from "react";
import { ChevronRight } from "lucide-react";
import { cn } from "../components/ui";
import type { RuleInfo } from "../generated/RuleInfo";
import { PriorityBadge } from "./priority";

/**
 * Above an issue's URLs (spec 009 FR-016, D10): its priority, the threshold
 * in force, why it matters and how to fix it. The explanation folds away;
 * the choice is remembered for the session.
 */
let explained = true;

export function IssueHeader({ rule, onSettings }: { rule: RuleInfo; onSettings?: () => void }) {
  const [open, setOpen] = useState(explained);
  const toggle = () => {
    explained = !open;
    setOpen(!open);
  };
  const t = rule.threshold;
  return (
    <div className="flex flex-col gap-1.5" data-testid="issue-header">
      <div className="flex flex-wrap items-center gap-2 text-sm text-fg-2">
        <PriorityBadge priority={rule.priority} />
        <span>{rule.category_label}</span>
        {t && (
          <span>
            · Threshold {t.value.toLocaleString()} {t.unit_label}
            {t.value !== t.default && <span className="text-fg-3"> (default {t.default.toLocaleString()})</span>}
          </span>
        )}
        {rule.kind === "site_wide" && <span className="text-fg-3">· Compares pages across the site</span>}
        {rule.standards.length > 0 && <span className="text-fg-3">· {rule.standards.join(", ")}</span>}
        {onSettings && (
          <button type="button" onClick={onSettings} className="text-accent-text hover:underline">
            Change in settings
          </button>
        )}
        <button type="button" aria-expanded={open} onClick={toggle} className="ml-auto flex items-center gap-1 text-fg-3 hover:text-fg-2">
          <ChevronRight aria-hidden className={cn("size-3 transition-transform duration-(--dur-fast)", open && "rotate-90")} />
          Why and how to fix
        </button>
      </div>
      {open && (
        <dl className="m-0 flex flex-col gap-1 text-sm">
          <div className="flex gap-3">
            <dt className="w-8 shrink-0 font-semibold text-fg-2">Why</dt>
            <dd className="m-0 text-fg">{rule.why}</dd>
          </div>
          <div className="flex gap-3">
            <dt className="w-8 shrink-0 font-semibold text-fg-2">Fix</dt>
            <dd className="m-0 text-fg">{rule.fix}</dd>
          </div>
        </dl>
      )}
    </div>
  );
}
