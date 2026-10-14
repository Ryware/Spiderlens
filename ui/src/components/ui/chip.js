import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { cn } from "./cn";
/** Filter chip with a live count (spec 007 filters, sidebar counts). */
export function Chip({ label, count, active, className, ...rest }) {
    return (_jsxs("button", { type: "button", "aria-pressed": active, className: cn("inline-flex h-6 items-center gap-1.5 rounded-full px-2.5 text-sm whitespace-nowrap", active ? "bg-accent-soft text-fg" : "text-fg-2 hover:bg-surface-2", className), ...rest, children: [label, count != null && (_jsx("span", { className: cn("tabular font-medium", active ? "text-accent-text" : "text-fg-3"), children: count.toLocaleString() }))] }));
}
