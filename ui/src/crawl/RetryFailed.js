import { jsxs as _jsxs, jsx as _jsx, Fragment as _Fragment } from "react/jsx-runtime";
import { useEffect, useState } from "react";
import { Button, Modal, Notice } from "../components/ui";
import { retryPhases } from "./useCrawlRun";
const RUNNING = "Wait until the crawl is paused or finished.";
const NO_FILE = "Run or open a crawl first.";
/**
 * Asks the engine whether the open crawl has failures worth retrying, again
 * whenever the crawl settles. While a crawl runs the answer is known without
 * asking.
 */
export function useRetryAvailability(transport, run) {
    const settled = retryPhases.includes(run.phase);
    const [preview, setPreview] = useState(null);
    useEffect(() => {
        setPreview(null);
        if (!run.path || !settled)
            return;
        let live = true;
        transport.retryPreview(run.path).then((p) => live && setPreview(p), (e) => live && setPreview({ total: 0, by_class: [], available: false, reason: String(e) }));
        return () => {
            live = false;
        };
    }, [transport, run.path, settled, run.report]);
    if (!run.path)
        return { available: false, total: 0, reason: NO_FILE };
    if (!settled)
        return { available: false, total: 0, reason: RUNNING };
    if (!preview)
        return { available: false, total: 0, reason: null };
    return { available: preview.available, total: preview.total, reason: preview.reason };
}
/**
 * Confirmation for "Retry failed URLs" (spec 004 US5.1, FR-031): how many
 * URLs, by error class, before anything changes. Counts are read fresh when
 * it opens.
 */
export function RetryDialog({ open, onOpenChange, transport, path, onConfirm, }) {
    const [preview, setPreview] = useState(null);
    const [error, setError] = useState(null);
    useEffect(() => {
        if (!open || !path)
            return;
        setPreview(null);
        setError(null);
        transport.retryPreview(path).then(setPreview, (e) => setError(String(e)));
    }, [open, path, transport]);
    const urls = (n) => `${n.toLocaleString()} ${n === 1 ? "URL" : "URLs"}`;
    return (_jsxs(Modal, { open: open, onOpenChange: onOpenChange, title: "Retry failed URLs", children: [error && (_jsxs(Notice, { kind: "bad", role: "alert", children: ["Could not count the failed URLs: ", error] })), !preview && !error && _jsx("p", { className: "m-0 text-sm text-fg-3", children: "Counting failed URLs\u2026" }), preview && !preview.available && _jsx(Notice, { kind: "info", children: preview.reason ?? "There is nothing to retry." }), preview?.available && (_jsxs(_Fragment, { children: [_jsxs("p", { className: "m-0 text-sm text-fg-2", children: [urls(preview.total), " failed for a reason that may have passed. Only these are requested again, each with a fresh set of attempts. Other errors stay as they are."] }), _jsx("div", { className: "overflow-hidden rounded-lg border border-line", children: _jsx("table", { "aria-label": "Failed URLs by error", className: "w-full table-fixed border-collapse text-sm", children: _jsx("tbody", { className: "divide-y divide-line", children: preview.by_class.map((c) => (_jsxs("tr", { "data-class": c.class, children: [_jsx("td", { className: "px-3 py-1.5", children: c.label }), _jsx("td", { className: "w-20 px-3 py-1.5 text-right tabular", children: c.count.toLocaleString() })] }, c.class))) }) }) })] })), _jsxs("div", { className: "flex justify-end gap-2", children: [_jsx(Button, { onClick: () => onOpenChange(false), children: preview?.available ? "Cancel" : "Close" }), preview?.available && (_jsxs(Button, { variant: "primary", onClick: () => {
                            onOpenChange(false);
                            onConfirm();
                        }, children: ["Retry ", urls(preview.total)] }))] })] }));
}
