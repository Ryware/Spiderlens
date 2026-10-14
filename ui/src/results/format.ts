// How cell values read (plan D8): numbers with separators, sizes and times
// with units, status codes with a meaning color.

import type { JsonValue } from "../generated/serde_json/JsonValue";
import type { Need } from "../generated/Need";
import type { ValueKind } from "../generated/ValueKind";

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB"];
  let v = n / 1024;
  let u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u++;
  }
  return `${v.toFixed(v < 10 ? 1 : 0)} ${units[u]}`;
}

/** A count in a few characters: 950, 1.2k, 34k, 1.5M. */
export function formatCompact(n: number): string {
  const units: [number, string][] = [
    [1e6, "M"],
    [1e3, "k"],
  ];
  for (const [size, unit] of units) {
    const v = n / size;
    if (v < 1) continue;
    const r = v < 10 ? Math.round(v * 10) / 10 : Math.round(v);
    // 999,950 rounds to 1000k: say 1M instead.
    if (r >= 1000 && unit === "k") return "1M";
    return `${r}${unit}`;
  }
  return String(n);
}

/** The text a cell shows; empty for no value. */
export function formatCell(kind: ValueKind, v: JsonValue): string {
  if (v === null || v === undefined) return "";
  switch (kind) {
    case "int":
      return typeof v === "number" ? v.toLocaleString() : String(v);
    case "bytes":
      return typeof v === "number" ? formatBytes(v) : String(v);
    case "ms":
      return typeof v === "number" ? `${v.toLocaleString()} ms` : String(v);
    case "bool":
      return v ? "Yes" : "No";
    case "score":
      // Whole numbers in the table; the detail pane and exports keep one decimal (spec 023 FR-004).
      return typeof v === "number" ? String(Math.round(v)) : String(v);
    case "percent":
      return typeof v === "number" ? `${v.toFixed(1)}%` : String(v);
    default:
      return String(v);
  }
}

export type CellTone = "ok" | "info" | "warn" | "bad" | null;

/** Meaning color for status codes and crawl states. */
export function cellTone(kind: ValueKind, v: JsonValue): CellTone {
  if (kind === "status" && typeof v === "number") {
    if (v < 300) return "ok";
    if (v < 400) return "info";
    if (v < 500) return "warn";
    return "bad";
  }
  if (kind === "state" && typeof v === "string") {
    if (v.startsWith("Failed")) return "bad";
    if (v.startsWith("Blocked")) return "warn";
  }
  if (kind === "text" && typeof v === "string" && v.startsWith("Non-indexable")) return "warn";
  return null;
}

export const toneText = { ok: "text-ok", info: "text-info", warn: "text-warn", bad: "text-bad" } as const;

export const numeric = (kind: ValueKind) => kind === "int" || kind === "bytes" || kind === "ms" || kind === "status" || kind === "score" || kind === "percent";

/** Why a column or filter has no data in this file. */
export const needReason: Record<Need, string> = {
  base: "",
  extraction: "Not collected: this crawl file was made before SpiderLens read page content.",
  attempts: "Not collected yet: retry attempts are recorded from spec 004.",
  timing: "Not collected: this crawl file was made before SpiderLens recorded response size and time.",
  raw_html: "This crawl did not keep page HTML.",
  analysis: "Calculated when the crawl ends.",
};
