import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { Gauge } from "lucide-react";
import { formatElapsed } from "../crawl/useCrawlRun";
import { Badge, Popover, cn } from "../components/ui";
import { siteName } from "./PausedSites";
/** Short status bar words for each limit; the popover has the full sentence. */
const limitLabel = {
    concurrency: "At your requests cap",
    rate: "At your URLs/s cap",
    per_site: "At your per-site cap",
    site_slowdown: "Site got slower",
    site_overload: "Site overloaded",
    requested_wait: "Site asked to wait",
    crawl_delay: "robots.txt crawl-delay",
    site_paused: "Site paused",
    starting_up: "Speeding up",
    connection: "Your connection is slow",
    computer: "Limited by this computer",
    saving: "Limited by saving",
};
/** The site's side of things is a warning; the user's own caps are not. */
const limitTone = {
    concurrency: "plain",
    rate: "plain",
    per_site: "plain",
    site_slowdown: "warn",
    site_overload: "warn",
    requested_wait: "warn",
    crawl_delay: "plain",
    site_paused: "warn",
    starting_up: "plain",
    connection: "warn",
    computer: "warn",
    saving: "warn",
};
const stateBadge = {
    speeding_up: { label: "Speeding up", tone: "info" },
    steady: { label: "Steady", tone: "ok" },
    slowed_down: { label: "Slowed down", tone: "warn" },
    waiting: { label: "Waiting", tone: "warn" },
    paused: { label: "Paused", tone: "bad" },
};
const count = (n) => n.toLocaleString("en-US");
/** The limit's label, with the site and the time left when it has them. */
function limitText(l) {
    const site = l.host ? ` · ${siteName(l.host)}` : "";
    const left = l.seconds_left !== null && l.seconds_left > 0 ? ` · ${formatElapsed(l.seconds_left * 1000)}` : "";
    return `${limitLabel[l.kind]}${site}${left}`;
}
function SiteRow({ s }) {
    const badge = stateBadge[s.state];
    const left = s.seconds_left !== null && s.seconds_left > 0 ? ` ${formatElapsed(s.seconds_left * 1000)}` : "";
    return (_jsxs("tr", { "data-site": s.host, className: "align-top", children: [_jsxs("td", { className: "max-w-48 px-3 py-1.5", children: [_jsx("div", { className: "truncate font-code text-fg", title: s.host, children: siteName(s.host) }), _jsx("div", { className: "text-xs whitespace-normal text-fg-3", "data-reason": true, children: s.reason })] }), _jsx("td", { className: "px-2 py-1.5", "data-state": s.state, children: _jsxs(Badge, { tone: badge.tone, children: [badge.label, left] }) }), _jsxs("td", { className: "px-2 py-1.5 text-right tabular", "data-col": "at-once", title: `${s.in_flight} open now, up to ${s.allowance} allowed, at most ${s.max}`, children: [s.allowance, " / ", s.max] }), _jsx("td", { className: "px-2 py-1.5 text-right tabular", "data-col": "response", children: s.response_ms === null ? "–" : `${count(s.response_ms)} ms` }), _jsx("td", { className: "px-2 py-1.5 text-right tabular", "data-col": "crawled", children: count(s.crawled) }), _jsxs("td", { className: "px-3 py-1.5 text-right tabular", "data-col": "waiting", children: [count(s.waiting), s.waiting_more ? "+" : ""] })] }));
}
/**
 * Status bar control for the crawl's speed (spec 008 FR-028, FR-029): what
 * keeps it from going faster, in a few words, and on click the full reason
 * with every site's pace.
 */
export function CrawlSpeed({ run }) {
    const limit = run.speedLimit;
    // A crawl waiting on paused sites already says so in the status bar.
    if (!run.running || run.waitingForHost || (!limit && run.sites.length === 0))
        return null;
    const warn = limit && limitTone[limit.kind] === "warn";
    const more = run.sitesTotal - run.sites.length;
    return (_jsxs(Popover, { title: "Crawl speed", side: "top", className: "w-150 sheet-max-w!", trigger: _jsxs("button", { type: "button", "data-figure": "speed", "data-limit": limit?.kind ?? "none", className: cn("inline-flex h-5 min-w-0 items-center gap-1 rounded-sm px-1 hover:bg-surface-2", warn ? "text-warn" : "text-fg-2"), children: [_jsx(Gauge, { "aria-hidden": true, className: "size-3 shrink-0" }), _jsx("span", { className: "truncate font-medium", children: limit ? limitText(limit) : "Full speed" })] }), children: [_jsx("p", { className: "m-0 border-b border-line px-3 py-2 text-sm text-fg-2", "data-limit-message": true, children: limit ? (limit.host ? limit.message.replaceAll(limit.host, siteName(limit.host)) : limit.message) : "Nothing is holding the crawl back: it runs as fast as your caps and the sites allow." }), run.sites.length > 0 && (_jsx("div", { className: "max-h-72 overflow-y-auto", children: _jsxs("table", { "aria-label": "Sites", className: "w-full border-collapse text-sm", children: [_jsx("thead", { className: "sticky top-0 bg-surface text-xs text-fg-3", children: _jsxs("tr", { children: [_jsx("th", { scope: "col", className: "px-3 py-1.5 text-left font-medium", children: "Site" }), _jsx("th", { scope: "col", className: "px-2 py-1.5 text-left font-medium", children: "State" }), _jsx("th", { scope: "col", className: "px-2 py-1.5 text-right font-medium", title: "Requests allowed at once now, and the most it gets", children: "At once" }), _jsx("th", { scope: "col", className: "px-2 py-1.5 text-right font-medium", children: "Response" }), _jsx("th", { scope: "col", className: "px-2 py-1.5 text-right font-medium", children: "Crawled" }), _jsx("th", { scope: "col", className: "px-3 py-1.5 text-right font-medium", children: "Waiting" })] }) }), _jsx("tbody", { className: "divide-y divide-line", children: run.sites.map((s) => (_jsx(SiteRow, { s: s }, s.host))) })] }) })), more > 0 && (_jsxs("p", { className: "m-0 border-t border-line px-3 py-2 text-xs text-fg-3", children: [count(more), " more ", more === 1 ? "site" : "sites", " with less work left."] }))] }));
}
