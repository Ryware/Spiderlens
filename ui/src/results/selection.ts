// Row selection as sorted, non-overlapping position ranges (FR-026). Ranges,
// not ids, so selecting a million rows costs nothing; copy and export send
// them to the engine as they are.

import type { Range } from "../transport";

export interface Selection {
  ranges: Range[];
  /** The row the keyboard is on. */
  focus: number | null;
  /** Where a Shift selection grows from. */
  anchor: number | null;
}

export const emptySelection: Selection = { ranges: [], focus: null, anchor: null };

/** Sorts and merges touching or overlapping ranges. */
export function normalize(ranges: Range[]): Range[] {
  const sorted = ranges.filter(([a, b]) => b > a).sort((x, y) => x[0] - y[0]);
  const out: Range[] = [];
  for (const [a, b] of sorted) {
    const last = out[out.length - 1];
    if (last && a <= last[1]) last[1] = Math.max(last[1], b);
    else out.push([a, b]);
  }
  return out;
}

export const count = (ranges: Range[]) => ranges.reduce((n, [a, b]) => n + b - a, 0);

export function contains(ranges: Range[], i: number): boolean {
  let lo = 0;
  let hi = ranges.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const [a, b] = ranges[mid];
    if (i < a) hi = mid - 1;
    else if (i >= b) lo = mid + 1;
    else return true;
  }
  return false;
}

/** Click: just this row. */
export function single(i: number): Selection {
  return { ranges: [[i, i + 1]], focus: i, anchor: i };
}

/** Shift+click or Shift+arrow: from the anchor to `i`, keeping other ranges added with Mod. */
export function extend(s: Selection, i: number): Selection {
  const anchor = s.anchor ?? i;
  const span: Range = [Math.min(anchor, i), Math.max(anchor, i) + 1];
  return { ranges: normalize([...s.ranges.filter((r) => !contains([r], anchor)), span]), focus: i, anchor };
}

/** Mod+click: adds or removes one row. */
export function toggle(s: Selection, i: number): Selection {
  if (!contains(s.ranges, i)) return { ranges: normalize([...s.ranges, [i, i + 1]]), focus: i, anchor: i };
  const ranges: Range[] = [];
  for (const [a, b] of s.ranges) {
    if (i >= a && i < b) {
      if (i > a) ranges.push([a, i]);
      if (i + 1 < b) ranges.push([i + 1, b]);
    } else ranges.push([a, b]);
  }
  return { ranges, focus: i, anchor: i };
}

export function all(total: number): Selection {
  return total === 0 ? emptySelection : { ranges: [[0, total]], focus: 0, anchor: 0 };
}

/** Moves the selection after a refresh moved its focused row by `delta`, clipped to the view. */
export function shift(s: Selection, delta: number, total: number): Selection {
  if (delta === 0) return clip(s, total);
  const move = (i: number | null) => (i === null ? null : i + delta);
  return clip({ ranges: s.ranges.map(([a, b]) => [a + delta, b + delta]), focus: move(s.focus), anchor: move(s.anchor) }, total);
}

export function clip(s: Selection, total: number): Selection {
  const ranges = normalize(s.ranges.map(([a, b]) => [Math.max(0, a), Math.min(total, b)]));
  const inView = (i: number | null) => (i === null || i < 0 || i >= total ? null : i);
  return { ranges, focus: inView(s.focus), anchor: inView(s.anchor) };
}
