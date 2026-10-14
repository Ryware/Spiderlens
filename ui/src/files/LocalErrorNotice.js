import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { HardDrive } from "lucide-react";
import { Button } from "../components/ui";
import { formatSize } from "./format";
export const localErrorTitle = {
    disk_full: "Disk is full",
    disk_low: "Disk space is low",
    drive_gone: "The drive was disconnected",
    permission: "Permission denied",
    network: "The network seems to be down",
    other: "Couldn't write the crawl file",
};
/**
 * A disk or write problem paused the crawl (spec 006 US5): what happened,
 * where the file is, and what to do. Never about the website.
 */
export function LocalErrorNotice({ error, onResume, onSaveAs, onKeepPaused }) {
    return (_jsxs("div", { role: "alert", "aria-labelledby": "local-error-title", className: "flex flex-col gap-2 rounded-lg border border-bad bg-bad-soft px-4 py-3 text-sm", children: [_jsxs("div", { className: "flex items-center gap-2", children: [_jsx(HardDrive, { "aria-hidden": true, className: "size-4 shrink-0 text-bad" }), _jsx("h2", { id: "local-error-title", className: "m-0 text-base font-semibold text-bad", children: localErrorTitle[error.kind] })] }), _jsxs("p", { className: "m-0 text-fg", children: [error.message, " The crawl is paused and nothing collected so far is lost. Fix the problem and resume, or save the crawl somewhere else."] }), _jsxs("dl", { className: "m-0 flex flex-col gap-0.5 text-fg-2", "data-selectable": true, children: [_jsxs("div", { className: "flex gap-3", children: [_jsx("dt", { className: "w-20 shrink-0 text-fg-3", children: "Crawl file" }), _jsx("dd", { className: "m-0 min-w-0 font-code break-all", children: error.path })] }), error.free_bytes !== null && (_jsxs("div", { className: "flex gap-3", children: [_jsx("dt", { className: "w-20 shrink-0 text-fg-3", children: "Free space" }), _jsx("dd", { className: "m-0 tabular", children: formatSize(error.free_bytes) })] }))] }), _jsxs("div", { className: "flex flex-wrap gap-2 pt-1", children: [_jsx(Button, { variant: "primary", size: "sm", onClick: onResume, children: "Resume" }), _jsx(Button, { size: "sm", onClick: onSaveAs, children: "Save As elsewhere\u2026" }), _jsx(Button, { size: "sm", variant: "quiet", onClick: onKeepPaused, children: "Keep paused" })] })] }));
}
