import { cn } from "./cn";
import { formatShortcut } from "../../shell/platform";

/** A keyboard shortcut, written the platform's way (⌘K on macOS, Ctrl+K elsewhere). */
export function Kbd({ keys, onAccent, className }: { keys: string; onAccent?: boolean; className?: string }) {
  return (
    <kbd
      className={cn(
        "inline-flex h-4.5 items-center rounded-sm border px-1 font-ui text-2xs tabular",
        onAccent ? "border-accent-fg/40 text-accent-fg" : "border-line text-fg-3",
        className,
      )}
    >
      {formatShortcut(keys)}
    </kbd>
  );
}
