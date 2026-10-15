/** A page to compare against. */
export interface CompareTarget {
  id: number;
  address: string;
  sim: number | null;
}

/**
 * What the comparison view shows: `base` against `targets[index]`; `[`
 * and `]` step through the targets (FR-020).
 */
export interface CompareRequest {
  base: { id: number; address: string };
  targets: CompareTarget[];
  index: number;
}

export type OpenCompare = (r: CompareRequest) => void;
