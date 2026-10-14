import { jsx as _jsx } from "react/jsx-runtime";
import { cn } from "./cn";
import { formatShortcut } from "../../shell/platform";
/** A keyboard shortcut, written the platform's way (⌘K on macOS, Ctrl+K elsewhere). */
export function Kbd({ keys, onAccent, className }) {
    return (_jsx("kbd", { className: cn("inline-flex h-4.5 items-center rounded-sm border px-1 font-ui text-2xs tabular", onAccent ? "border-accent-fg/40 text-accent-fg" : "border-line text-fg-3", className), children: formatShortcut(keys) }));
}
