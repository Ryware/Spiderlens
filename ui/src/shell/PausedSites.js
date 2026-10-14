import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { CirclePause } from "lucide-react";
import { formatElapsed } from "../crawl/useCrawlRun";
import { Badge, Button, Popover } from "../components/ui";
/** example.com for https://example.com:443; the port only when it is unusual. */
export function siteName(hostKey) {
    const m = /^(https?):\/\/(.+?)(?::(\d+))?$/.exec(hostKey);
    if (!m)
        return hostKey;
    const [, scheme, host, port] = m;
    const usual = !port || (scheme === "https" && port === "443") || (scheme === "http" && port === "80");
    return usual ? host : `${host}:${port}`;
}
/** Time left on a pause, or what the site is doing instead. */
function pauseState(h) {
    if (h.given_up)
        return _jsx(Badge, { tone: "bad", children: "Given up" });
    if (h.probing || h.seconds_left === 0)
        return _jsx("span", { className: "text-fg-2", children: "Testing\u2026" });
    return _jsx("span", { className: "text-fg-2", children: formatElapsed(h.seconds_left * 1000) });
}
/**
 * Status bar control for sites the crawl has paused because they kept failing
 * (spec 004 FR-028): a count that opens the list, each with its reason, the
 * time left and "Resume now" (US4.5).
 */
export function PausedSites({ run }) {
    const hosts = run.pausedHosts;
    if (hosts.length === 0)
        return null;
    const paused = hosts.filter((h) => !h.given_up).length;
    const gaveUp = hosts.length - paused;
    const label = [paused > 0 && `${paused} ${paused === 1 ? "site" : "sites"} paused`, gaveUp > 0 && `${gaveUp} given up`].filter(Boolean).join(" · ");
    return (_jsxs(Popover, { title: "Paused sites", side: "top", trigger: _jsxs("button", { type: "button", "data-figure": "paused", className: "inline-flex h-5 items-center gap-1 rounded-sm px-1 text-warn hover:bg-surface-2", children: [_jsx(CirclePause, { "aria-hidden": true, className: "size-3" }), _jsx("span", { className: "font-medium", children: label })] }), children: [_jsx("ul", { "aria-label": "Paused sites", className: "m-0 flex max-h-64 list-none flex-col divide-y divide-line overflow-y-auto p-0", children: hosts.map((h) => (_jsxs("li", { "data-host": h.host, className: "flex items-center gap-3 px-3 py-2", children: [_jsxs("div", { className: "min-w-0 flex-1", children: [_jsx("div", { className: "truncate font-code text-sm text-fg", title: h.host, children: siteName(h.host) }), _jsx("div", { className: "truncate text-xs text-fg-3", title: h.reason, children: h.reason })] }), _jsx("span", { className: "shrink-0 text-xs tabular", "data-pause-state": true, children: pauseState(h) }), !h.given_up && (_jsx(Button, { size: "sm", disabled: h.probing, onClick: () => run.resumeHost(h.host), "aria-label": `Resume ${siteName(h.host)} now`, children: "Resume now" }))] }, h.host))) }), _jsx("p", { className: "m-0 border-t border-line px-3 py-2 text-xs text-fg-3", children: "Other sites keep being crawled. A paused site gets one test request when its time is up." })] }));
}
