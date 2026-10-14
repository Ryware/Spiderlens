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

export const numeric = (kind: ValueKind) => kind === "int" || kind === "bytes" || kind === "ms" || kind === "status";

/** Why a column or filter has no data in this file. */
export const needReason: Record<Need, string> = {
  base: "",
  extraction: "Not collected: this crawl file was made before SpiderLens read page content.",
  attempts: "Not collected yet: retry attempts are recorded from spec 004.",
  timing: "Not collected: this crawl file was made before SpiderLens recorded response size and time.",
  raw_html: "This crawl did not keep page HTML.",
  rendering: "This crawl was not rendered.",
  rendered_html: "This crawl does not store rendered HTML.",
  screenshots: "This crawl did not take screenshots.",
};
