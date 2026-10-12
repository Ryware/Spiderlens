import type { ButtonHTMLAttributes } from "react";
import { cn } from "./cn";

/** Filter chip with a live count (spec 007 filters, sidebar counts). */
export function Chip({
  label,
  count,
  active,
  className,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { label: string; count?: number | null; active?: boolean }) {
  return (
    <button
      type="button"
      aria-pressed={active}
      className={cn(
        "inline-flex h-6 items-center gap-1.5 rounded-full px-2.5 text-sm whitespace-nowrap",
        active ? "bg-accent-soft text-fg" : "text-fg-2 hover:bg-surface-2",
        className,
      )}
      {...rest}
    >
      {label}
      {count != null && (
        <span className={cn("tabular font-medium", active ? "text-accent-text" : "text-fg-3")}>{count.toLocaleString()}</span>
      )}
    </button>
  );
}
