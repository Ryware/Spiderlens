import { Plus, X } from "lucide-react";
import type { Issue } from "../generated/Issue";
import type { RewriteRule } from "../generated/RewriteRule";
import { Button, Input } from "../components/ui";
import { IssueList, issuesAt } from "./fields";

/** URL rewrite rules: a regex and its replacement per row, applied in order (spec 002 follow-up). */
export function RewriteEditor({ rules, onChange, issues = [] }: { rules: RewriteRule[]; onChange: (r: RewriteRule[]) => void; issues?: Issue[] }) {
  const update = (i: number, r: RewriteRule) => onChange(rules.map((x, j) => (j === i ? r : x)));
  return (
    <div className="flex w-full flex-col gap-2">
      {rules.map((r, i) => (
        <div className="flex flex-col gap-1" key={i}>
          <div className="flex flex-wrap items-center gap-2">
            <Input aria-label={`Rewrite ${i + 1} pattern`} placeholder="Pattern, e.g. ^http://" mono className="min-w-0 flex-1" value={r.pattern} onChange={(e) => update(i, { ...r, pattern: e.target.value })} />
            <Input aria-label={`Rewrite ${i + 1} replacement`} placeholder="Replacement, e.g. https://" mono className="min-w-0 flex-1" value={r.replace} onChange={(e) => update(i, { ...r, replace: e.target.value })} />
            <Button variant="quiet" icon size="sm" aria-label={`Remove rewrite ${i + 1}`} onClick={() => onChange(rules.filter((_, j) => j !== i))}>
              <X className="size-3.5" />
            </Button>
          </div>
          <IssueList issues={issuesAt(issues, `scope.rewrite.${i}`)} />
        </div>
      ))}
      <div>
        <Button size="sm" onClick={() => onChange([...rules, { pattern: "", replace: "" }])}>
          <Plus className="size-3.5" /> Add rule
        </Button>
      </div>
    </div>
  );
}
