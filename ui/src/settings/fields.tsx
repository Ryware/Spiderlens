import { useId, type ReactNode } from "react";
import type { Issue } from "../generated/Issue";
import { cn, Notice } from "../components/ui";

/** Issues whose path is `prefix` or nested under it. */
export function issuesAt(issues: Issue[], prefix: string): Issue[] {
  return issues.filter(
    (i) => i.path === prefix || i.path.startsWith(prefix + ".") || i.path.startsWith(prefix + "["),
  );
}

/** Errors and warnings in the row they belong to (FR-011). Warnings are always visible. */
export function IssueList({ issues, className }: { issues: Issue[]; className?: string }) {
  if (issues.length === 0) return null;
  return (
    <ul className={cn("m-0 flex list-none flex-col gap-1 p-0", className)}>
      {issues.map((i) => (
        <li key={i.path + i.code} data-code={i.code} role={i.severity === "error" ? "alert" : "status"}>
          <Notice kind={i.severity === "error" ? "bad" : "warn"}>
            <span className="sr-only">{i.severity === "error" ? "Error: " : "Warning: "}</span>
            {i.message}
          </Notice>
        </li>
      ))}
    </ul>
  );
}

/** A titled group of rows, drawn as one rounded box (the macOS grouped form). */
export function Group({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-1.5">
      {title && <h3 className="m-0 px-1 text-xs font-semibold text-fg-3">{title}</h3>}
      <div className="divide-y divide-line overflow-hidden rounded-lg border border-line bg-surface">{children}</div>
    </section>
  );
}

/**
 * One setting: label on the left, control on the right, an optional line of
 * help and the engine's issues underneath (FR-010, FR-011).
 */
export function Row({
  label,
  help,
  issues = [],
  children,
  stacked,
}: {
  label: string;
  help?: ReactNode;
  issues?: Issue[];
  /** Renders the control; native controls take `id`, switches take `labelId`. */
  children: (id: string, labelId: string) => ReactNode;
  /** Put the control under the label (lists, editors). */
  stacked?: boolean;
}) {
  const id = useId();
  return (
    <div className="flex flex-col gap-1.5 px-3.5 py-2.5">
      <div className={cn("flex gap-4", stacked ? "flex-col gap-2" : "items-center justify-between")}>
        <label htmlFor={id} id={`${id}-label`} className="min-w-0 text-base text-fg">
          {label}
        </label>
        {children(id, `${id}-label`)}
      </div>
      {help && <div className="text-sm text-fg-3">{help}</div>}
      <IssueList issues={issues} />
    </div>
  );
}
