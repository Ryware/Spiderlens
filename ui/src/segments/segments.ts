// Segment helpers shared by the settings editor and the results filter
// (spec 027 part B). The engine decides membership; this file only words
// and colours things.

import type { CSSProperties } from "react";
import type { Segment } from "../generated/Segment";
import type { SegmentClause } from "../generated/SegmentClause";
import type { SegmentColor } from "../generated/SegmentColor";
import type { SegmentField } from "../generated/SegmentField";
import type { SegmentOp } from "../generated/SegmentOp";

export const SEGMENT_COLORS: { value: SegmentColor; label: string }[] = [
  { value: "teal", label: "Teal" },
  { value: "blue", label: "Blue" },
  { value: "violet", label: "Violet" },
  { value: "pink", label: "Pink" },
  { value: "orange", label: "Orange" },
  { value: "amber", label: "Amber" },
  { value: "green", label: "Green" },
  { value: "gray", label: "Gray" },
];

/** The dot colour, from the segment tokens in tokens.css. */
export const segmentDot = (c: SegmentColor): CSSProperties => ({ background: `var(--seg-${c})` });

export const MAX_SEGMENTS = 100;
export const MAX_CLAUSES = 10;

export const FIELDS: { value: SegmentField; label: string; kind: "text" | "number" | "choice" }[] = [
  { value: "path", label: "Path", kind: "text" },
  { value: "url", label: "URL", kind: "text" },
  { value: "host", label: "Host", kind: "text" },
  { value: "query", label: "Query", kind: "text" },
  { value: "status_class", label: "Status class", kind: "choice" },
  { value: "content_kind", label: "Content type", kind: "choice" },
  { value: "indexability", label: "Indexability", kind: "choice" },
  { value: "language", label: "Language", kind: "text" },
  { value: "depth", label: "Crawl depth", kind: "number" },
  { value: "word_count", label: "Word count", kind: "number" },
];

/** Values a choice field can take, as the engine names them. */
export const CHOICES: Partial<Record<SegmentField, { value: string; label: string }[]>> = {
  status_class: ["2xx", "3xx", "4xx", "5xx"].map((v) => ({ value: v, label: v })).concat([
    { value: "other", label: "Other" },
    { value: "none", label: "No response" },
  ]),
  content_kind: [
    { value: "html", label: "HTML" },
    { value: "image", label: "Image" },
    { value: "pdf", label: "PDF" },
    { value: "css", label: "CSS" },
    { value: "javascript", label: "JavaScript" },
    { value: "other", label: "Other" },
  ],
  indexability: [
    { value: "indexable", label: "Indexable" },
    { value: "non-indexable", label: "Non-indexable" },
  ],
};

const TEXT_OPS: { value: SegmentOp; label: string }[] = [
  { value: "contains", label: "contains" },
  { value: "not_contains", label: "does not contain" },
  { value: "starts_with", label: "starts with" },
  { value: "ends_with", label: "ends with" },
  { value: "equals", label: "is" },
  { value: "not_equals", label: "is not" },
  { value: "matches", label: "matches pattern" },
];
const NUMBER_OPS: { value: SegmentOp; label: string }[] = [
  { value: "lt", label: "<" },
  { value: "le", label: "≤" },
  { value: "equals", label: "=" },
  { value: "ge", label: "≥" },
  { value: "gt", label: ">" },
  { value: "not_equals", label: "≠" },
];
const CHOICE_OPS: { value: SegmentOp; label: string }[] = [
  { value: "equals", label: "is" },
  { value: "not_equals", label: "is not" },
];

export function fieldKind(f: SegmentField) {
  return FIELDS.find((x) => x.value === f)?.kind ?? "text";
}

export function opsFor(f: SegmentField) {
  const k = fieldKind(f);
  return k === "number" ? NUMBER_OPS : k === "choice" ? CHOICE_OPS : TEXT_OPS;
}

/** A clause moved to another field keeps what still makes sense. */
export function withField(c: SegmentClause, field: SegmentField): SegmentClause {
  const ops = opsFor(field);
  const op = ops.some((o) => o.value === c.op) ? c.op : ops[0].value;
  const choices = CHOICES[field];
  const value = choices ? (choices.some((x) => x.value === c.value) ? c.value : choices[0].value) : fieldKind(field) === fieldKind(c.field) ? c.value : "";
  return { field, op, value };
}

export const newClause = (): SegmentClause => ({ field: "path", op: "starts_with", value: "" });

/** A new segment with the next free id, name and colour. */
export function newSegment(existing: Segment[]): Segment {
  const id = Math.max(0, ...existing.map((s) => s.id)) + 1;
  const taken = new Set(existing.map((s) => s.name.trim().toLowerCase()));
  let n = existing.length + 1;
  while (taken.has(`segment ${n}`)) n++;
  return {
    id,
    name: `Segment ${n}`,
    color: SEGMENT_COLORS[(id - 1) % SEGMENT_COLORS.length].value,
    enabled: true,
    mode: "all",
    clauses: [newClause()],
  };
}

export const enabledSegments = (s: Segment[] | null | undefined) => (s ?? []).filter((x) => x.enabled);
