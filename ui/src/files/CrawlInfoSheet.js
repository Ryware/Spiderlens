import { jsx as _jsx, jsxs as _jsxs, Fragment as _Fragment } from "react/jsx-runtime";
import { useEffect, useState } from "react";
import { Button, Notice, Sheet, SheetClose } from "../components/ui";
import { EntryBadges } from "./RecentCrawls";
import { formatDateTime, formatSize } from "./format";
const robots = { respect: "Respected", ignore_report: "Ignored, blocked URLs reported", ignore: "Ignored" };
const versionVerb = { created: "Created", resumed: "Resumed", upgraded: "Upgraded" };
const noLimit = (n) => (n === null ? "No limit" : n.toLocaleString());
const list = (xs) => (xs.length ? xs.join("\n") : "None");
/** The stored configuration as label and value, for reading (spec 006 US6-1). Secrets never appear. */
export function configRows(c) {
    const s = c.speed;
    const ua = c.identity.user_agent;
    const rows = [
        ["Mode", c.mode === "spider" ? "Spider" : "List"],
        ["URL limit", noLimit(c.limits.max_urls)],
        c.mode === "spider" ? ["Depth limit", noLimit(c.limits.max_depth)] : ["Follow links from listed URLs", c.limits.list_depth === 0 ? "No" : `${c.limits.list_depth} clicks`],
        ["Include", list(c.scope.include)],
        ["Exclude", list(c.scope.exclude)],
        ["Subdomains", c.scope.subdomains === "this_host" ? "This host only" : "All subdomains"],
        ["Check external links", c.scope.check_external ? "Yes" : "No"],
        ["Speed", `${s.max_concurrent} at once, ${s.max_per_host} per host, ${s.max_urls_per_sec === null ? "no rate limit" : `up to ${s.max_urls_per_sec} URLs/s`}${s.adaptive ? ", slows down when the site does" : ""}`],
        ["robots.txt", `${robots[c.robots.mode]}${c.robots.honour_crawl_delay ? ", crawl-delay honoured" : ""}`],
        ["User agent", ua.preset === "custom" ? (ua.custom_string ?? "Custom") : ua.preset],
        ["Retries", `${c.retry.attempts} attempts, up to ${c.retry.max_wait_secs} s apart`],
        ["Timeouts", `Connect ${c.network.connect_timeout_secs} s, read ${c.network.read_timeout_secs} s`],
    ];
    for (const h of c.identity.headers)
        rows.push([`Header ${h.name}`, "plain" in h.value ? h.value.plain : "secret, not stored"]);
    return rows;
}
/**
 * Crawl information (spec 006 US6): what the file is, how it was made and
 * with which settings, so results can be explained and repeated.
 */
export function CrawlInfoSheet({ open, onOpenChange, transport, path, onNewCrawl, }) {
    const [info, setInfo] = useState(null);
    const [error, setError] = useState(null);
    useEffect(() => {
        if (!open || !path)
            return;
        let live = true;
        setError(null);
        transport.crawlInfo(path).then((i) => live && setInfo(i), (e) => live && setError(String(e)));
        return () => {
            live = false;
        };
    }, [open, path, transport]);
    const i = info?.path === path ? info : null;
    return (_jsxs(Sheet, { open: open, onOpenChange: onOpenChange, title: "Crawl information", className: "w-160 sheet-max-w", children: [_jsxs("header", { className: "flex items-center gap-2 border-b border-line px-5 py-3", children: [_jsx("h2", { className: "m-0 min-w-0 truncate text-md font-semibold", children: i?.name ?? "Crawl information" }), i && _jsx(EntryBadges, { entry: i })] }), _jsxs("div", { className: "flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto px-5 py-4", "data-selectable": true, children: [error && (_jsxs(Notice, { kind: "bad", role: "alert", children: ["Could not read the crawl information: ", error] })), !i && !error && _jsx("p", { className: "m-0 text-sm text-fg-3", children: "Reading\u2026" }), i && (_jsxs(_Fragment, { children: [_jsx(Section, { title: "File", children: _jsx(Pairs, { rows: [
                                        ["Location", _jsx("span", { className: "font-code break-all", children: i.path })],
                                        ["Size", formatSize(i.size_bytes)],
                                        ["Format", `Version ${i.format_version}`],
                                        ["Status", i.pause_reason ? `${statusText(i)} (${i.pause_reason})` : statusText(i)],
                                        ["Mode", i.mode === "spider" ? "Spider" : "List"],
                                        ...(i.start_url ? [["Start URL", _jsx("span", { className: "font-code break-all", children: i.start_url })]] : []),
                                        ["Started", formatDateTime(i.started_at)],
                                        ["Ended", formatDateTime(i.ended_at)],
                                    ] }) }), _jsx(Section, { title: "URLs", children: _jsx(Pairs, { rows: [
                                        ["Discovered", i.counts.discovered.toLocaleString()],
                                        ["Crawled", i.counts.crawled.toLocaleString()],
                                        ["Failed", i.counts.failed.toLocaleString()],
                                        ["Not crawled", i.counts.not_crawled.toLocaleString()],
                                        ["Queued", i.counts.queued.toLocaleString()],
                                    ] }) }), _jsx(Section, { title: "Versions", children: _jsx("ul", { "aria-label": "Versions", className: "m-0 flex list-none flex-col gap-1 p-0 text-sm", children: i.versions.map((v, n) => (_jsxs("li", { className: "flex gap-2", children: [_jsxs("span", { className: "text-fg", children: [versionVerb[v.kind], " with SpiderLens ", v.app_version] }), _jsxs("span", { className: "text-fg-3", children: ["on ", v.os] }), _jsx("span", { className: "ml-auto text-fg-3 tabular", children: formatDateTime(v.at) })] }, n))) }) }), i.events.length > 0 && (_jsx(Section, { title: "History", children: _jsx("ul", { "aria-label": "History", className: "m-0 flex list-none flex-col gap-1 p-0 text-sm", children: i.events.map((e, n) => (_jsxs("li", { className: "flex gap-2", children: [_jsx("span", { className: "w-36 shrink-0 text-fg-3 tabular", children: formatDateTime(e.at) }), _jsxs("span", { className: "min-w-0 flex-1 text-fg", children: [e.kind[0].toUpperCase() + e.kind.slice(1).replace(/_/g, " "), e.detail ? _jsxs("span", { className: "text-fg-2", children: [": ", e.detail] }) : null] }), _jsx("span", { className: "shrink-0 text-fg-3", children: e.app_version })] }, n))) }) })), _jsx(Section, { title: "Configuration", children: i.config ? _jsx(Pairs, { rows: configRows(i.config), label: "Configuration" }) : _jsx("p", { className: "m-0 text-sm text-fg-3", children: "This file has no configuration SpiderLens can read." }) })] }))] }), _jsxs("footer", { className: "flex justify-end gap-2 border-t border-line px-5 py-3", children: [_jsx(Button, { disabled: !i?.config, onClick: () => i && onNewCrawl(i), children: "New crawl with these settings" }), _jsx(SheetClose, { render: _jsx(Button, { variant: "primary" }), children: "Close" })] })] }));
}
const statusText = (i) => ({ running: "Running", paused: "Paused", stopped: "Stopped (incomplete)", completed: "Completed" })[i.status];
function Section({ title, children }) {
    return (_jsxs("section", { className: "flex flex-col gap-1.5", children: [_jsx("h3", { className: "m-0 text-xs font-semibold text-fg-3", children: title }), children] }));
}
function Pairs({ rows, label }) {
    return (_jsx("dl", { "aria-label": label, className: "m-0 flex flex-col gap-1 text-sm", children: rows.map(([k, v]) => (_jsxs("div", { className: "flex gap-4", children: [_jsx("dt", { className: "w-40 shrink-0 text-fg-3", children: k }), _jsx("dd", { className: "m-0 min-w-0 flex-1 whitespace-pre-line text-fg", children: v })] }, k))) }));
}
