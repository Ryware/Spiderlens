import type { ReactNode } from "react";
import type { Issue } from "../generated/Issue";

/** Issues whose path is `prefix` or nested under it. */
export function issuesAt(issues: Issue[], prefix: string): Issue[] {
  return issues.filter(
    (i) => i.path === prefix || i.path.startsWith(prefix + ".") || i.path.startsWith(prefix + "["),
  );
}

/** Errors and warnings next to a setting. Warnings are always visible (SC-009). */
export function IssueList({ issues }: { issues: Issue[] }) {
  if (issues.length === 0) return null;
  return (
    <ul className="issues">
      {issues.map((i) => (
        <li
          key={i.path + i.code}
          className={i.severity}
          role={i.severity === "error" ? "alert" : "status"}
          data-code={i.code}
        >
          <strong>{i.severity === "error" ? "Error" : "Warning"}:</strong> {i.message}
        </li>
      ))}
    </ul>
  );
}

export function Field({ label, children, hint }: { label: string; children: ReactNode; hint?: string }) {
  return (
    <label className="field">
      <span className="label">{label}</span>
      {children}
      {hint && <span className="hint">{hint}</span>}
    </label>
  );
}

/** Number input where blank means `null` (for example "no limit"). */
export function OptionalNumber({
  label,
  value,
  onChange,
  disabled,
  hint,
  step,
}: {
  label: string;
  value: number | null;
  onChange: (v: number | null) => void;
  disabled?: boolean;
  hint?: string;
  step?: number;
}) {
  return (
    <Field label={label} hint={hint}>
      <input
        type="number"
        aria-label={label}
        value={value ?? ""}
        step={step ?? 1}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value === "" ? null : Number(e.target.value))}
      />
    </Field>
  );
}

export function NumberField({
  label,
  value,
  onChange,
  step,
}: {
  label: string;
  value: number;
  onChange: (v: number) => void;
  step?: number;
}) {
  return (
    <Field label={label}>
      <input
        type="number"
        aria-label={label}
        value={Number.isNaN(value) ? "" : value}
        step={step ?? 1}
        onChange={(e) => onChange(e.target.value === "" ? NaN : Number(e.target.value))}
      />
    </Field>
  );
}

export function Toggle({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label className="toggle">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} /> {label}
    </label>
  );
}

export function Section({ title, children, open }: { title: string; children: ReactNode; open?: boolean }) {
  return (
    <details className="section" open={open}>
      <summary>{title}</summary>
      <div className="section-body">{children}</div>
    </details>
  );
}

/** One pattern per line. */
export function Lines({
  label,
  value,
  onChange,
  placeholder,
}: {
  label: string;
  value: string[];
  onChange: (v: string[]) => void;
  placeholder?: string;
}) {
  return (
    <Field label={label} hint="One regular expression per line.">
      <textarea
        aria-label={label}
        rows={3}
        placeholder={placeholder}
        value={value.join("\n")}
        onChange={(e) => onChange(e.target.value === "" ? [] : e.target.value.split("\n"))}
      />
    </Field>
  );
}
