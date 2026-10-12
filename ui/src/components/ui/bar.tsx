import { cn } from "./cn";
import type { Tone } from "./badge";

const fills: Record<Tone, string> = { ok: "bg-ok", info: "bg-info", warn: "bg-warn", bad: "bg-bad", neutral: "bg-fg-3" };

/** Inline proportion bar for table cells and progress (0..1). */
export function Bar({ value, tone = "neutral", className, label }: { value: number; tone?: Tone | "accent"; className?: string; label?: string }) {
  const v = Math.max(0, Math.min(1, value));
  return (
    <span
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(v * 100)}
      className={cn("relative block h-1 w-12 overflow-hidden rounded-full bg-line", className)}
    >
      <span
        className={cn("absolute inset-y-0 left-0 rounded-full", tone === "accent" ? "bg-accent" : fills[tone])}
        style={{ width: `${v * 100}%` }}
      />
    </span>
  );
}
