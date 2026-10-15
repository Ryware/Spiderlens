import type { Tone } from "../components/ui";
import type { IssuePriority as Priority } from "../generated/IssuePriority";

export const PRIORITY_LABEL: Record<Priority, string> = {
  high: "High",
  medium: "Medium",
  low: "Low",
  opportunity: "Opportunity",
};

export const PRIORITY_TONE: Record<Priority, Tone> = {
  high: "bad",
  medium: "warn",
  low: "info",
  opportunity: "neutral",
};

/** The same bands as the report file (crates/results/src/report/html.rs). */
export function scoreBand(value: number): { label: string; tone: Tone } {
  if (value >= 90) return { label: "Good", tone: "ok" };
  if (value >= 70) return { label: "Needs work", tone: "warn" };
  return { label: "Poor", tone: "bad" };
}

/** Score points with one decimal, without a trailing ".0". */
export const points = (n: number) => (Math.round(n * 10) / 10).toLocaleString();

/** A file name from the crawl's host, safe on every OS. */
export function reportName(startUrl: string | null): string {
  const host = startUrl?.replace(/^https?:\/\//, "").split(/[/?#:]/)[0] ?? "";
  return `${host || "crawl"}-report`.replace(/[^a-z0-9.-]+/gi, "-");
}
