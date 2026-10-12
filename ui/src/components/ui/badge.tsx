import type { ReactNode } from "react";
import { cn } from "./cn";

export type Tone = "ok" | "info" | "warn" | "bad" | "neutral";

const tones: Record<Tone, string> = {
  ok: "text-ok bg-ok-soft",
  info: "text-info bg-info-soft",
  warn: "text-warn bg-warn-soft",
  bad: "text-bad bg-bad-soft",
  neutral: "text-fg-2 bg-surface-2",
};

/** Status pill: a meaning color is always paired with its text label (FR-017). */
export function Badge({ tone = "neutral", children, className }: { tone?: Tone; children: ReactNode; className?: string }) {
  return (
    <span
      className={cn(
        "inline-flex h-4.5 items-center gap-1 rounded-full px-1.75 text-xs font-semibold whitespace-nowrap tabular",
        "before:size-1.5 before:rounded-full before:bg-current",
        tones[tone],
        className,
      )}
    >
      {children}
    </span>
  );
}
