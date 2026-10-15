import type { SegmentCounts } from "../generated/SegmentCounts";
import { Select } from "../components/ui";
import { enabledSegments, segmentDot } from "./segments";

/**
 * The segment filter in every results tab (spec 027 FR-B04): the choice is
 * shared by all tabs, and each segment shows how many URLs it holds.
 * `value`: a segment id, 0 for "No segment", null for all URLs.
 */
export function SegmentFilter({ counts, value, onChange }: { counts: SegmentCounts | null; value: number | null; onChange: (v: number | null) => void }) {
  const segments = enabledSegments(counts?.segments);
  if (segments.length === 0) return null;
  const current = segments.find((s) => s.id === value);
  const n = (x: number | undefined) => (x === undefined ? "" : ` (${x.toLocaleString()})`);
  return (
    <span className="flex items-center gap-1.5">
      {current && <span aria-hidden data-testid="segment-dot" className="size-2 shrink-0 rounded-full" style={segmentDot(current.color)} />}
      <Select
        aria-label="Segment"
        className="h-6 w-44 [&>select]:h-6 [&>select]:text-sm"
        value={value === null ? "" : String(value)}
        onChange={(e) => onChange(e.target.value === "" ? null : Number(e.target.value))}
      >
        <option value="">All segments</option>
        {segments.map((s) => (
          <option key={s.id} value={s.id}>
            {s.name.trim()}
            {n(counts?.counts[s.id])}
          </option>
        ))}
        <option value="0">No segment{n(counts?.no_segment)}</option>
      </Select>
    </span>
  );
}
