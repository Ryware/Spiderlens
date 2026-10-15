// Settings › Segments (spec 027 FR-B01, FR-B02, FR-B07): name, colour, on or
// off, order, and the conditions of each segment. The first enabled segment
// whose conditions match a URL is its segment; the engine checks each change
// and its issues show next to the condition that caused them.

import { ArrowDown, ArrowUp, Plus, Trash2, X } from "lucide-react";
import type { Issue } from "../generated/Issue";
import type { Segment } from "../generated/Segment";
import type { SegmentClause } from "../generated/SegmentClause";
import type { SegmentColor } from "../generated/SegmentColor";
import type { SegmentField } from "../generated/SegmentField";
import type { SegmentMatch } from "../generated/SegmentMatch";
import type { SegmentOp } from "../generated/SegmentOp";
import { Button, Input, Segmented, Select, Switch } from "../components/ui";
import { IssueList, issuesAt } from "../settings/fields";
import { CHOICES, FIELDS, MAX_CLAUSES, MAX_SEGMENTS, SEGMENT_COLORS, fieldKind, newClause, newSegment, opsFor, segmentDot, withField } from "./segments";

function ClauseRow({
  clause,
  label,
  issues,
  onChange,
  onRemove,
}: {
  clause: SegmentClause;
  label: string;
  issues: Issue[];
  onChange: (c: SegmentClause) => void;
  onRemove: (() => void) | null;
}) {
  const choices = CHOICES[clause.field];
  const kind = fieldKind(clause.field);
  return (
    <li className="flex flex-col gap-1" data-clause>
      <div className="flex flex-wrap items-center gap-1.5">
        <Select aria-label={`${label}: field`} className="w-36" value={clause.field} onChange={(e) => onChange(withField(clause, e.target.value as SegmentField))}>
          {FIELDS.map((f) => (
            <option key={f.value} value={f.value}>
              {f.label}
            </option>
          ))}
        </Select>
        <Select aria-label={`${label}: test`} className="w-40" value={clause.op} onChange={(e) => onChange({ ...clause, op: e.target.value as SegmentOp })}>
          {opsFor(clause.field).map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </Select>
        {choices ? (
          <Select aria-label={`${label}: value`} className="w-40" value={clause.value} onChange={(e) => onChange({ ...clause, value: e.target.value })}>
            {choices.map((c) => (
              <option key={c.value} value={c.value}>
                {c.label}
              </option>
            ))}
          </Select>
        ) : (
          <Input
            aria-label={`${label}: value`}
            aria-invalid={issues.some((i) => i.severity === "error") || undefined}
            className={kind === "number" ? "w-24" : "min-w-40 flex-1"}
            mono={kind === "text"}
            inputMode={kind === "number" ? "numeric" : undefined}
            placeholder={clause.op === "matches" ? "^/blog/\\d+" : kind === "number" ? "0" : clause.field === "language" ? "en" : "/blog/"}
            value={clause.value}
            onChange={(e) => onChange({ ...clause, value: e.target.value })}
          />
        )}
        {onRemove && (
          <Button size="sm" variant="quiet" icon aria-label={`Remove ${label.toLowerCase()}`} onClick={onRemove}>
            <X aria-hidden className="size-3.5" />
          </Button>
        )}
      </div>
      <IssueList issues={issues} />
    </li>
  );
}

function SegmentCard({
  segment,
  index,
  count,
  issues,
  onChange,
  onMove,
  onRemove,
}: {
  segment: Segment;
  index: number;
  count: number;
  issues: Issue[];
  onChange: (s: Segment) => void;
  onMove: (dir: -1 | 1) => void;
  onRemove: () => void;
}) {
  const at = `segments[${index}]`;
  const name = segment.name.trim() || `Segment ${index + 1}`;
  const setClause = (j: number, c: SegmentClause) => onChange({ ...segment, clauses: segment.clauses.map((x, k) => (k === j ? c : x)) });
  const own = issues.filter((i) => !i.path.startsWith(`${at}.clauses[`) && i.path !== `${at}.name`);
  return (
    <section aria-label={name} data-segment={segment.id} className="flex flex-col gap-2.5 rounded-lg border border-line bg-surface px-3.5 py-3">
      <div className="flex items-center gap-2">
        <span aria-hidden className="size-2.5 shrink-0 rounded-full" style={segmentDot(segment.color)} />
        <Input
          aria-label="Segment name"
          aria-invalid={issuesAt(issues, `${at}.name`).length > 0 || undefined}
          className="min-w-0 flex-1 font-medium"
          value={segment.name}
          onChange={(e) => onChange({ ...segment, name: e.target.value })}
        />
        <Select aria-label={`${name}: colour`} className="w-28" value={segment.color} onChange={(e) => onChange({ ...segment, color: e.target.value as SegmentColor })}>
          {SEGMENT_COLORS.map((c) => (
            <option key={c.value} value={c.value}>
              {c.label}
            </option>
          ))}
        </Select>
        <Switch label={`${name}: on`} checked={segment.enabled} onChange={(enabled) => onChange({ ...segment, enabled })} />
        <Button size="sm" variant="quiet" icon aria-label={`Move ${name} up`} disabled={index === 0} onClick={() => onMove(-1)}>
          <ArrowUp aria-hidden className="size-3.5" />
        </Button>
        <Button size="sm" variant="quiet" icon aria-label={`Move ${name} down`} disabled={index === count - 1} onClick={() => onMove(1)}>
          <ArrowDown aria-hidden className="size-3.5" />
        </Button>
        <Button size="sm" variant="quiet" icon aria-label={`Delete ${name}`} onClick={onRemove}>
          <Trash2 aria-hidden className="size-3.5" />
        </Button>
      </div>
      <IssueList issues={issuesAt(issues, `${at}.name`)} />
      <div className="flex items-center gap-2 text-sm text-fg-2">
        URLs that match
        <Segmented<SegmentMatch>
          size="sm"
          label={`${name}: match`}
          value={segment.mode}
          onChange={(mode) => onChange({ ...segment, mode })}
          options={[
            { value: "all", label: "all of" },
            { value: "any", label: "any of" },
          ]}
        />
        these conditions
      </div>
      <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
        {segment.clauses.map((c, j) => (
          <ClauseRow
            key={j}
            clause={c}
            label={`${name} condition ${j + 1}`}
            issues={issuesAt(issues, `${at}.clauses[${j}]`)}
            onChange={(c) => setClause(j, c)}
            onRemove={segment.clauses.length > 1 ? () => onChange({ ...segment, clauses: segment.clauses.filter((_, k) => k !== j) }) : null}
          />
        ))}
      </ul>
      <IssueList issues={own} />
      <div>
        <Button size="sm" variant="quiet" disabled={segment.clauses.length >= MAX_CLAUSES} onClick={() => onChange({ ...segment, clauses: [...segment.clauses, newClause()] })}>
          <Plus aria-hidden className="size-3.5" />
          Add condition
        </Button>
      </div>
    </section>
  );
}

export function SegmentsSettings({ segments, issues, onChange }: { segments: Segment[]; issues: Issue[]; onChange: (s: Segment[]) => void }) {
  const move = (i: number, dir: -1 | 1) => {
    const next = [...segments];
    [next[i], next[i + dir]] = [next[i + dir], next[i]];
    onChange(next);
  };
  return (
    <div className="flex flex-col gap-3">
      <p className="m-0 text-sm text-fg-2">
        Each URL goes in the first segment, from the top, whose conditions it matches. Segments apply to the open crawl at once and are saved with the profile.
      </p>
      <IssueList issues={issues.filter((i) => i.path === "segments")} />
      {segments.length === 0 && <p className="m-0 rounded-lg border border-dashed border-line px-3.5 py-6 text-center text-sm text-fg-3">No segments yet. Add one to group URLs, for example the blog or the shop.</p>}
      {segments.map((s, i) => (
        <SegmentCard
          key={s.id}
          segment={s}
          index={i}
          count={segments.length}
          issues={issuesAt(issues, `segments[${i}]`)}
          onChange={(x) => onChange(segments.map((y, k) => (k === i ? x : y)))}
          onMove={(dir) => move(i, dir)}
          onRemove={() => onChange(segments.filter((_, k) => k !== i))}
        />
      ))}
      <div>
        <Button variant="secondary" disabled={segments.length >= MAX_SEGMENTS} onClick={() => onChange([...segments, newSegment(segments)])}>
          <Plus aria-hidden className="size-3.5" />
          Add segment
        </Button>
      </div>
    </div>
  );
}
