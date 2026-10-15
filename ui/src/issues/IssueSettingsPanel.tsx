import { useEffect, useMemo, useState } from "react";
import { Search } from "lucide-react";
import { Button, Input, Notice, NumberInput, Select, Switch } from "../components/ui";
import type { IssueSettings } from "../generated/IssueSettings";
import type { IssuePriority as Priority } from "../generated/IssuePriority";
import type { RuleCatalogue } from "../generated/RuleCatalogue";
import type { RuleInfo } from "../generated/RuleInfo";
import type { RuleOverride } from "../generated/RuleOverride";
import type { Transport } from "../transport";
import { Group } from "../settings/fields";
import { NO_OVERRIDES } from "./useIssues";
import { PRIORITIES, priorityLabel } from "./priority";

const EMPTY: RuleOverride = { enabled: null, priority: null, threshold: null };

/**
 * Returns `s` with one rule's override changed. Only values that differ
 * from the catalogue are kept, so the profile stores overrides only (D7).
 */
export function withOverride(s: IssueSettings, rule: RuleInfo, patch: Partial<RuleOverride>): IssueSettings {
  const o: RuleOverride = { ...EMPTY, ...s.rules[rule.id], ...patch };
  if (o.enabled === true) o.enabled = null;
  if (o.priority === rule.default_priority) o.priority = null;
  if (rule.threshold && o.threshold === rule.threshold.default) o.threshold = null;
  const rules = { ...s.rules };
  if (o.enabled === null && o.priority === null && o.threshold === null) delete rules[rule.id];
  else rules[rule.id] = o;
  return { rules };
}

/**
 * Settings › Issues (spec 009 US4, FR-021 to FR-024): every rule by
 * category, with a switch, a priority and its threshold. Values out of
 * range are reported and not applied.
 */
export function IssueSettingsPanel({ transport, settings, onChange }: { transport: Transport; settings: IssueSettings | undefined; onChange: (s: IssueSettings) => void }) {
  const s = settings ?? NO_OVERRIDES;
  const key = JSON.stringify(s);
  const [catalogue, setCatalogue] = useState<RuleCatalogue | null>(null);
  const [query, setQuery] = useState("");

  useEffect(() => {
    let off = false;
    transport.ruleCatalogue(JSON.parse(key) as IssueSettings).then((c) => !off && setCatalogue(c), () => {});
    return () => {
      off = true;
    };
  }, [transport, key]);

  const groups = useMemo(() => {
    const q = query.trim().toLowerCase();
    const out = new Map<string, RuleInfo[]>();
    for (const r of catalogue?.rules ?? []) {
      if (q && !`${r.name} ${r.id} ${r.category_label}`.toLowerCase().includes(q)) continue;
      out.set(r.category_label, [...(out.get(r.category_label) ?? []), r]);
    }
    return [...out];
  }, [catalogue, query]);

  if (!catalogue) return <p className="m-0 text-sm text-fg-3">Loading rules…</p>;
  const errors = catalogue.notices.filter((n) => n.error);
  const changed = Object.keys(s.rules).length;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center gap-2">
        <label className="relative flex flex-1 items-center">
          <Search aria-hidden className="pointer-events-none absolute left-2 size-3.5 text-fg-3" />
          <Input type="search" aria-label="Search rules" placeholder="Search rules" value={query} onChange={(e) => setQuery(e.target.value)} className="h-control w-full pl-7" />
        </label>
        <Button size="sm" variant="quiet" disabled={changed === 0} onClick={() => onChange(NO_OVERRIDES)}>
          Reset to defaults
        </Button>
      </div>
      {catalogue.notices.length > 0 && (
        <Notice kind={errors.length > 0 ? "bad" : "warn"}>
          {errors.length > 0 && "These values are not applied until they are fixed. "}
          {catalogue.notices.map((n) => `${n.rule}: ${n.message}`).join(" ")}
        </Notice>
      )}
      {groups.length === 0 && <p className="m-0 text-sm text-fg-3">No rule matches “{query}”.</p>}
      {groups.map(([category, rules]) => (
        <Group key={category} title={category}>
          {rules.map((r) => (
            <RuleRow key={r.id} rule={r} override={s.rules[r.id]} onChange={(patch) => onChange(withOverride(s, r, patch))} />
          ))}
        </Group>
      ))}
    </div>
  );
}

function RuleRow({ rule, override, onChange }: { rule: RuleInfo; override: RuleOverride | undefined; onChange: (patch: Partial<RuleOverride>) => void }) {
  const labelId = `rule-${rule.id}`;
  const t = rule.threshold;
  // Show what was typed even when it is out of range (the notice says why).
  const threshold = override?.threshold ?? t?.value ?? null;
  return (
    <div className="flex flex-col gap-1.5 px-3.5 py-2.5" data-testid={`rule-${rule.id}`}>
      <div className="flex items-center gap-3">
        <Switch labelledBy={labelId} checked={rule.enabled} onChange={(v) => onChange({ enabled: v })} />
        <span id={labelId} className="min-w-0 flex-1 text-base text-fg">
          {rule.name}
        </span>
        {t && (
          <span className="flex items-center gap-1 text-sm text-fg-3">
            <NumberInput
              aria-label={`${rule.name} threshold`}
              value={threshold}
              min={t.min}
              max={t.max}
              disabled={!rule.enabled}
              onChange={(v) => onChange({ threshold: v ?? t.default })}
              className="w-20"
            />
            {t.unit_label}
          </span>
        )}
        <Select aria-label={`${rule.name} priority`} value={rule.priority} disabled={!rule.enabled} onChange={(e) => onChange({ priority: e.target.value as Priority })} className="w-32">
          {PRIORITIES.map((p) => (
            <option key={p} value={p}>
              {priorityLabel[p]}
            </option>
          ))}
        </Select>
      </div>
      <p className="m-0 text-sm text-fg-3">{rule.why}</p>
    </div>
  );
}
