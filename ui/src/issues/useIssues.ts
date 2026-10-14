import { useEffect, useMemo, useRef, useState } from "react";
import type { IssueSettings } from "../generated/IssueSettings";
import type { IssuesSummary } from "../generated/IssuesSummary";
import type { RuleInfo } from "../generated/RuleInfo";
import type { Transport } from "../transport";

export const NO_OVERRIDES: IssueSettings = { rules: {} };

/** How often counts refresh while a crawl runs (plan D5: provisional counts). */
const LIVE_REFRESH_MS = 3000;

/**
 * The open crawl's issues (spec 009 US1): the rule catalogue with the
 * profile's settings, and each rule's count. Applies the settings to the
 * engine first, so counts, issue tabs and exports agree (FR-023).
 * `applied` counts accepted applies (each refetches the counts); `changed`
 * counts only those that changed the settings, so views keyed on it reload
 * with the new thresholds but not when a crawl file opens.
 */
export function useIssues(transport: Transport, path: string | null, running: boolean, settings: IssueSettings | undefined) {
  const s = settings ?? NO_OVERRIDES;
  const key = JSON.stringify(s);
  const [rules, setRules] = useState<RuleInfo[]>([]);
  const [summary, setSummary] = useState<IssuesSummary | null>(null);
  const [applied, setApplied] = useState(0);
  const [changed, setChanged] = useState(0);
  const lastKey = useRef<string | null>(null);

  useEffect(() => {
    let off = false;
    const current = JSON.parse(key) as IssueSettings;
    transport.ruleCatalogue(current).then((c) => !off && setRules(c.rules), () => {});
    transport.issuesApply(path, current).then(
      (notices) => {
        if (off || notices.some((n) => n.error)) return;
        setApplied((n) => n + 1);
        if (lastKey.current !== null && lastKey.current !== key) setChanged((n) => n + 1);
        lastKey.current = key;
      },
      () => {},
    );
    return () => {
      off = true;
    };
  }, [transport, path, key]);

  useEffect(() => {
    setSummary(null);
  }, [path]);

  useEffect(() => {
    if (!path || applied === 0) return;
    let off = false;
    const load = () =>
      transport.issuesSummary(path, running).then(
        (r) => !off && setSummary(r),
        () => {},
      );
    load();
    const timer = running ? setInterval(load, LIVE_REFRESH_MS) : undefined;
    return () => {
      off = true;
      if (timer) clearInterval(timer);
    };
  }, [transport, path, running, applied]);

  const byId = useMemo(() => new Map(rules.map((r) => [r.id, r])), [rules]);
  return { rules, byId, summary, applied, changed };
}

export type Issues = ReturnType<typeof useIssues>;

/** The results tab of one issue's URLs (D10). */
export const issueTab = (id: string) => `issue:${id}`;
export const issueOfTab = (tab: string) => (tab.startsWith("issue:") ? tab.slice(6) : null);
