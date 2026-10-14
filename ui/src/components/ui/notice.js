import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { AlertTriangle, CircleX, Info } from "lucide-react";
import { cn } from "./cn";
const icons = { info: Info, warn: AlertTriangle, bad: CircleX };
const colors = { info: "text-info", warn: "text-warn", bad: "text-bad" };
/** One-line message with a meaning icon and color (never color alone). */
export function Notice({ kind = "info", children, className, role }) {
    const Icon = icons[kind];
    return (_jsxs("div", { role: role, className: cn("flex items-start gap-1.5 text-sm", kind === "info" ? "text-fg-2" : colors[kind], className), children: [_jsx(Icon, { "aria-hidden": true, className: cn("mt-px size-3.5 shrink-0", colors[kind]) }), _jsx("div", { className: "min-w-0", children: children })] }));
}
export function EmptyState({ icon, title, children }) {
    return (_jsxs("div", { className: "flex h-full flex-col items-center justify-center gap-2 p-6 text-center", children: [icon && _jsx("div", { className: "text-fg-3", children: icon }), _jsx("div", { className: "text-base font-semibold text-fg", children: title }), children && _jsx("div", { className: "max-w-72 text-sm text-fg-3", children: children })] }));
}
