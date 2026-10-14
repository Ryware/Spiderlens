import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { useId } from "react";
import { cn, Notice } from "../components/ui";
/** Issues whose path is `prefix` or nested under it. */
export function issuesAt(issues, prefix) {
    return issues.filter((i) => i.path === prefix || i.path.startsWith(prefix + ".") || i.path.startsWith(prefix + "["));
}
/** Errors and warnings in the row they belong to (FR-011). Warnings are always visible. */
export function IssueList({ issues, className }) {
    if (issues.length === 0)
        return null;
    return (_jsx("ul", { className: cn("m-0 flex list-none flex-col gap-1 p-0", className), children: issues.map((i) => (_jsx("li", { "data-code": i.code, role: i.severity === "error" ? "alert" : "status", children: _jsxs(Notice, { kind: i.severity === "error" ? "bad" : "warn", children: [_jsx("span", { className: "sr-only", children: i.severity === "error" ? "Error: " : "Warning: " }), i.message] }) }, i.path + i.code))) }));
}
/**
 * A titled group of rows, drawn as one rounded box (the macOS grouped form).
 * `disabled` locks every control in it, as while a crawl runs.
 */
export function Group({ title, children, disabled }) {
    return (_jsxs("section", { className: "flex flex-col gap-1.5", children: [title && _jsx("h3", { className: "m-0 px-1 text-xs font-semibold text-fg-3", children: title }), _jsx("fieldset", { disabled: disabled, className: "m-0 min-w-0 divide-y divide-line overflow-hidden rounded-lg border border-line bg-surface p-0", children: children })] }));
}
/**
 * One setting: label on the left, control on the right, an optional line of
 * help and the engine's issues underneath (FR-010, FR-011).
 */
export function Row({ label, help, issues = [], children, stacked, }) {
    const id = useId();
    return (_jsxs("div", { className: "flex flex-col gap-1.5 px-3.5 py-2.5", children: [_jsxs("div", { className: cn("flex gap-4", stacked ? "flex-col gap-2" : "items-center justify-between"), children: [_jsx("label", { htmlFor: id, id: `${id}-label`, className: "min-w-0 text-base text-fg", children: label }), children(id, `${id}-label`)] }), help && _jsx("div", { className: "text-sm text-fg-3", children: help }), _jsx(IssueList, { issues: issues })] }));
}
