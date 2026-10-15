import { useState } from "react";
import { ArrowDown, ArrowUp, Plus, X } from "lucide-react";
import type { Issue } from "../generated/Issue";
import type { LinkAnalysisSettings } from "../generated/LinkAnalysisSettings";
import type { Position } from "../generated/Position";
import type { PositionRule } from "../generated/PositionRule";
import type { PositionWeights } from "../generated/PositionWeights";
import type { TestedLink } from "../generated/TestedLink";
import type { Transport } from "../transport";
import { Badge, Button, Input, Notice, NumberInput, Select, Switch } from "../components/ui";
import { Group, IssueList, issuesAt, Row } from "./fields";

export const positions: { value: Position; label: string }[] = [
  { value: "navigation", label: "Navigation" },
  { value: "header", label: "Header" },
  { value: "content", label: "Content" },
  { value: "sidebar", label: "Sidebar" },
  { value: "footer", label: "Footer" },
];

const positionLabel = (p: Position) => positions.find((x) => x.value === p)?.label ?? p;

/** Spec 023 defaults (FR-025). */
export const defaultWeights: PositionWeights = { navigation: 0.3, header: 0.3, content: 1, sidebar: 0.5, footer: 0.2, unknown: 1 };

const weightRows: { key: keyof PositionWeights; label: string; help?: string }[] = [
  { key: "content", label: "Content" },
  { key: "sidebar", label: "Sidebar" },
  { key: "header", label: "Header" },
  { key: "navigation", label: "Navigation" },
  { key: "footer", label: "Footer" },
  { key: "unknown", label: "Unknown", help: "Links on pages whose HTML was not stored." },
];

/** Ordered custom rules: selector, position, move and remove (FR-021). */
export function PositionRulesEditor({ rules, issues, onChange }: { rules: PositionRule[]; issues: Issue[]; onChange: (r: PositionRule[]) => void }) {
  const update = (i: number, r: PositionRule) => onChange(rules.map((x, j) => (j === i ? r : x)));
  const move = (i: number, by: number) => {
    const next = [...rules];
    [next[i], next[i + by]] = [next[i + by], next[i]];
    onChange(next);
  };
  return (
    <div className="flex w-full flex-col gap-2">
      {rules.length === 0 && <div className="text-sm text-fg-3">No custom rules. The built-in rules use the page's nav, header, aside and footer elements.</div>}
      {rules.map((r, i) => (
        <div key={i} className="flex flex-col gap-1">
          <div className="flex items-center gap-2">
            <span className="w-5 shrink-0 text-right text-xs text-fg-3 tabular">{i + 1}</span>
            <Input
              aria-label={`Rule ${i + 1} selector`}
              placeholder=".mega-menu"
              mono
              className="min-w-0 flex-1"
              value={r.selector}
              onChange={(e) => update(i, { ...r, selector: e.target.value })}
            />
            <Select aria-label={`Rule ${i + 1} position`} className="w-32" value={r.position} onChange={(e) => update(i, { ...r, position: e.target.value as Position })}>
              {positions.map((p) => (
                <option key={p.value} value={p.value}>
                  {p.label}
                </option>
              ))}
            </Select>
            <Button variant="quiet" icon size="sm" aria-label={`Move rule ${i + 1} up`} disabled={i === 0} onClick={() => move(i, -1)}>
              <ArrowUp className="size-3.5" />
            </Button>
            <Button variant="quiet" icon size="sm" aria-label={`Move rule ${i + 1} down`} disabled={i === rules.length - 1} onClick={() => move(i, 1)}>
              <ArrowDown className="size-3.5" />
            </Button>
            <Button variant="quiet" icon size="sm" aria-label={`Remove rule ${i + 1}`} onClick={() => onChange(rules.filter((_, j) => j !== i))}>
              <X className="size-3.5" />
            </Button>
          </div>
          <IssueList issues={issuesAt(issues, `links.position_rules[${i}]`)} className="pl-7" />
        </div>
      ))}
      <div>
        <Button size="sm" onClick={() => onChange([...rules, { selector: "", position: "navigation" }])}>
          <Plus className="size-3.5" /> Add rule
        </Button>
      </div>
    </div>
  );
}

/** Runs the rules on one crawled page and shows each link's position (FR-022). */
export function PositionTester({ transport, path, rules }: { transport: Transport; path: string | null; rules: PositionRule[] }) {
  const [url, setUrl] = useState("");
  const [links, setLinks] = useState<TestedLink[] | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  if (!path) return <div className="text-sm text-fg-3">Open a crawl to try the rules on one of its pages.</div>;
  const run = async () => {
    setError(null);
    try {
      setLinks(await transport.positionsTest(path, url, rules));
    } catch (e) {
      setError(String(e));
    }
  };
  return (
    <div className="flex w-full flex-col gap-2">
      <form
        className="flex items-center gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void run();
        }}
      >
        <Input aria-label="Page URL" placeholder="https://example.com/page" mono className="min-w-0 flex-1" value={url} onChange={(e) => setUrl(e.target.value)} />
        <Button size="sm" type="submit" disabled={url.trim() === ""}>
          Test rules
        </Button>
      </form>
      {error && <Notice kind="bad">{error}</Notice>}
      {links === null && <div className="text-sm text-fg-3">That URL is not in this crawl, or its HTML was not stored.</div>}
      {links && links.length === 0 && <div className="text-sm text-fg-3">The page has no links.</div>}
      {links && links.length > 0 && (
        <table className="w-full border-collapse text-sm">
          <thead>
            <tr className="text-left text-xs text-fg-3">
              <th className="py-1 pr-2 font-medium">Link</th>
              <th className="py-1 pr-2 font-medium">Position</th>
              <th className="py-1 font-medium">Decided by</th>
            </tr>
          </thead>
          <tbody>
            {links.map((l) => (
              <tr key={l.pos} className="border-t border-line">
                <td className="max-w-60 truncate py-1 pr-2" data-selectable title={l.raw}>
                  <span className="font-code">{l.raw}</span>
                  {l.text && <span className="text-fg-3"> {l.text}</span>}
                </td>
                <td className="py-1 pr-2">
                  <Badge tone={l.position === "content" ? "ok" : "neutral"}>{positionLabel(l.position)}</Badge>
                </td>
                <td className="py-1 text-fg-2">{l.rule === null ? "Built-in rule" : `Rule ${l.rule + 1}`}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

/** Settings › Link analysis (spec 023 plan D9). */
export function LinkAnalysisBody({
  links,
  issues,
  onChange,
  transport,
  path,
}: {
  links: LinkAnalysisSettings;
  issues: Issue[];
  onChange: (l: LinkAnalysisSettings) => void;
  transport: Transport;
  path: string | null;
}) {
  const weights = links.weighting;
  const setWeight = (key: keyof PositionWeights, v: number | null) => weights && onChange({ ...links, weighting: { ...weights, [key]: v ?? NaN } });
  return (
    <>
      <Group>
        <Row label="Work out Link Score when a crawl ends" help="Runs after fetching, so it never slows the crawl.">
          {(_, l) => <Switch labelledBy={l} checked={links.auto_analyse} onChange={(v) => onChange({ ...links, auto_analyse: v })} />}
        </Row>
      </Group>
      <Group title="Link positions">
        <Row
          label="Custom rules"
          stacked
          help="A CSS selector for an element around the link, and the position it means. Checked in order before the built-in rules; the first match wins."
          issues={issuesAt(issues, "links.position_rules").filter((i) => i.path === "links.position_rules")}
        >
          {() => <PositionRulesEditor rules={links.position_rules} issues={issues} onChange={(position_rules) => onChange({ ...links, position_rules })} />}
        </Row>
        <Row label="Try the rules" stacked help="Shows where each link on a crawled page sits, and which rule decided. Nothing is saved.">
          {() => <PositionTester transport={transport} path={path} rules={links.position_rules} />}
        </Row>
      </Group>
      <Group title="Weighting">
        <Row label="Weight Link Score by link position" help="Links in menus and footers repeated on every page then count for less. Off gives the classic score.">
          {(_, l) => <Switch labelledBy={l} checked={weights !== null} onChange={(v) => onChange({ ...links, weighting: v ? defaultWeights : null })} />}
        </Row>
        {weights &&
          weightRows.map((w) => (
            <Row key={w.key} label={w.label} help={w.help} issues={issuesAt(issues, `links.weighting.${w.key}`)}>
              {(i) => <NumberInput id={i} step={0.1} min={0} max={1} value={weights[w.key]} onChange={(v) => setWeight(w.key, v)} />}
            </Row>
          ))}
        {weights && (
          <div className="px-3.5 py-2.5">
            <Button size="sm" onClick={() => onChange({ ...links, weighting: defaultWeights })}>
              Reset weights
            </Button>
          </div>
        )}
      </Group>
    </>
  );
}
