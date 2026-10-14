import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
// The detail pane for the selected URL (US3, FR-019..FR-025).
import { useEffect, useState } from "react";
import { CornerDownRight, MousePointerClick } from "lucide-react";
import { Badge, Button, cn, EmptyState, Notice, Segmented } from "../components/ui";
import { cellTone, formatBytes, toneText } from "./format";
const LIST_PAGE = 100;
const stateTone = (d) => {
    if (d.error_class || d.final_state.startsWith("Failed"))
        return "bad";
    const t = cellTone("status", d.status);
    if (t)
        return t;
    return d.not_crawled_reason ? "warn" : "neutral";
};
const fieldLabels = {
    indexability: "Indexability",
    headers: "Response headers",
    headings: "Headings",
    anchor_text: "Anchor and alt text",
    size: "Size",
    response_time: "Response time",
    attempts: "Retry attempts",
    raw_html: "Page HTML",
};
function Section({ title, children, aside }) {
    return (_jsxs("section", { className: "flex flex-col gap-1.5 border-b border-line px-3 py-3", children: [_jsxs("div", { className: "flex items-center gap-2", children: [_jsx("h3", { className: "m-0 text-xs font-semibold text-fg-3", children: title }), aside && _jsx("div", { className: "ml-auto", children: aside })] }), children] }));
}
function Facts({ rows }) {
    return (_jsx("dl", { className: "m-0 grid grid-cols-3 gap-x-3 gap-y-1 text-sm", "data-selectable": true, children: rows.map(([k, v]) => (_jsxs("div", { className: "contents", children: [_jsx("dt", { className: "text-fg-3", children: k }), _jsx("dd", { className: "col-span-2 m-0 min-w-0 break-words text-fg", children: v })] }, k))) }));
}
/** Anchor text, or for images whether the alt text is there. */
function LinkText({ kind, text }) {
    if (kind === "images") {
        if (text === null)
            return _jsx("span", { className: "shrink-0 text-xs text-warn", children: "No alt attribute" });
        if (text === "")
            return _jsx("span", { className: "shrink-0 text-xs text-warn", children: "Empty alt" });
    }
    if (!text)
        return null;
    return (_jsx("span", { className: "max-w-2/5 shrink-0 truncate text-fg-3", title: text, children: text }));
}
function LinkList({ transport, path, id, kind, total }) {
    const [page, setPage] = useState(null);
    const [error, setError] = useState(null);
    useEffect(() => {
        let off = false;
        setPage(null);
        setError(null);
        transport.urlList(path, id, kind, 0, LIST_PAGE).then((p) => !off && setPage(p), (e) => !off && setError(String(e)));
        return () => {
            off = true;
        };
    }, [transport, path, id, kind]);
    const more = () => page &&
        transport.urlList(path, id, kind, page.rows.length, LIST_PAGE).then((p) => setPage({ ...p, from: 0, rows: [...page.rows, ...p.rows], headings: [...page.headings, ...p.headings] }), (e) => setError(String(e)));
    if (error)
        return _jsx(Notice, { kind: "bad", children: error });
    if (!page)
        return _jsx("div", { className: "text-sm text-fg-3", children: "Loading\u2026" });
    if (page.unavailable)
        return _jsx("div", { className: "text-sm text-fg-3", children: page.unavailable });
    const shown = kind === "headings" ? page.headings.length : page.rows.length;
    if (shown === 0)
        return _jsx("div", { className: "text-sm text-fg-3", children: "None." });
    return (_jsxs("div", { className: "flex flex-col", children: [kind === "headings" ? (_jsx("ol", { className: "m-0 flex list-none flex-col p-0", "data-selectable": true, children: page.headings.map((h, i) => (_jsxs("li", { className: "flex items-baseline gap-2 border-b border-line py-1 text-sm last:border-b-0", children: [_jsxs("span", { className: "w-6 shrink-0 text-xs font-medium text-fg-3", children: ["H", h.level] }), _jsx("span", { className: cn("min-w-0 flex-1 break-words", h.level === 1 ? "text-fg" : "text-fg-2"), children: h.text || _jsx("span", { className: "text-fg-3", children: "(empty)" }) })] }, i))) })) : (_jsx("ul", { className: "m-0 flex list-none flex-col p-0", "data-selectable": true, children: page.rows.map((r, i) => {
                    const tone = cellTone("status", r.status);
                    return (_jsxs("li", { className: "flex items-baseline gap-2 border-b border-line py-1 text-sm last:border-b-0", children: [_jsx("span", { className: cn("w-8 shrink-0 text-right tabular", tone ? toneText[tone] : "text-fg-3"), children: r.status ?? "–" }), _jsx("span", { className: "min-w-0 flex-1 truncate", title: r.url, children: r.url }), _jsx(LinkText, { kind: kind, text: r.text }), (r.nofollow || !r.followed || r.sponsored || r.ugc) && (_jsx("span", { className: "shrink-0 text-xs text-fg-3", children: [r.nofollow ? "nofollow" : !r.followed ? "not followed" : null, r.sponsored && "sponsored", r.ugc && "ugc"].filter(Boolean).join(", ") }))] }, `${r.id}-${i}`));
                }) })), shown < total && (_jsxs(Button, { size: "sm", variant: "quiet", className: "mt-1 self-start", onClick: () => void more(), children: ["Show more (", (total - shown).toLocaleString(), " left)"] }))] }));
}
export function Inspector({ transport, path, id }) {
    const [detail, setDetail] = useState(null);
    const [error, setError] = useState(null);
    const [list, setList] = useState("inlinks");
    useEffect(() => {
        if (!path || id === null) {
            setDetail(null);
            return;
        }
        let off = false;
        setError(null);
        transport.urlDetail(path, id).then((d) => !off && setDetail(d), (e) => !off && setError(String(e)));
        return () => {
            off = true;
        };
    }, [transport, path, id]);
    if (!path || id === null) {
        return (_jsx("aside", { "aria-label": "Details", className: "h-full overflow-y-auto bg-surface", children: _jsx(EmptyState, { icon: _jsx(MousePointerClick, { className: "size-6" }), title: "Nothing selected", children: "Select a URL in the results to see its response, redirects and links here." }) }));
    }
    if (error) {
        return (_jsx("aside", { "aria-label": "Details", className: "h-full overflow-y-auto bg-surface p-3", children: _jsx(Notice, { kind: "bad", children: error }) }));
    }
    if (!detail || detail.id !== id)
        return _jsx("aside", { "aria-label": "Details", className: "h-full bg-surface", "aria-busy": true });
    const d = detail;
    const tone = stateTone(d);
    const facts = [
        ["Status", d.status !== null ? `${d.status}${d.status_text ? ` ${d.status_text}` : ""}` : "No response"],
        ["Content type", d.content_type ?? "–"],
        ["Depth", d.depth.toLocaleString()],
        ["Found on", d.found_on ? _jsx("span", { className: "break-all", children: d.found_on }) : "Start URL"],
        ["Discovered via", d.discovered_via],
        ["In sitemap", d.in_sitemap ? "Yes" : "No"],
    ];
    if (d.size !== null)
        facts.push(["Size", formatBytes(d.size)]);
    if (d.response_ms !== null)
        facts.push(["Response time", `${d.response_ms.toLocaleString()} ms`]);
    if (d.attempts !== null && (d.attempts > 1 || d.manual_retry)) {
        facts.push(["Attempts", `${d.attempts}${d.manual_retry ? ", last by Retry failed URLs" : ""}`]);
    }
    if (d.indexability) {
        const non = d.indexability.startsWith("Non-indexable");
        facts.push(["Indexability", _jsx("span", { className: non ? "text-warn" : undefined, children: d.indexability })]);
    }
    if (d.fetched_at !== null)
        facts.push(["Fetched", new Date(d.fetched_at).toLocaleString()]);
    const counts = { inlinks: d.inlinks, outlinks: d.outlinks, images: d.images, headings: d.headings };
    // Fields the file has no data for, grouped by why.
    const missing = new Map();
    for (const [k, why] of Object.entries(d.unavailable))
        missing.set(why, [...(missing.get(why) ?? []), fieldLabels[k] ?? k]);
    return (_jsxs("aside", { "aria-label": "Details", className: "flex h-full flex-col overflow-y-auto bg-surface", children: [_jsxs("header", { className: "flex flex-col gap-2 border-b border-line px-3 py-3", children: [_jsxs("div", { className: "flex items-center gap-2", children: [_jsx(Badge, { tone: tone, children: d.final_state }), !d.internal && _jsx(Badge, { tone: "neutral", children: "External" })] }), _jsx("h2", { className: "m-0 text-base font-medium break-all text-fg", "data-selectable": true, title: d.url, children: d.url_readable })] }), (d.error_label || d.not_crawled_reason) && (_jsx(Section, { title: d.error_label ? "Error" : "Not crawled", children: _jsx(Notice, { kind: d.error_label ? "bad" : "warn", children: _jsxs("span", { "data-selectable": true, children: [d.error_label ?? d.not_crawled_reason, d.error_message ? `: ${d.error_message}` : d.reason_detail ? `: ${d.reason_detail}` : "", d.robots_rule ? ` (rule ${d.robots_rule})` : ""] }) }) })), _jsx(Section, { title: "Response", children: _jsx(Facts, { rows: facts }) }), d.redirects.length > 0 && (_jsxs(Section, { title: `Redirects (${d.redirects.length - 1} ${d.redirects.length === 2 ? "hop" : "hops"})`, children: [d.redirect_loop && _jsx(Notice, { kind: "bad", children: "The chain loops back on itself." }), d.redirect_over_limit && _jsx(Notice, { kind: "warn", children: "The chain is longer than the crawl's redirect limit." }), _jsx("ol", { className: "m-0 flex list-none flex-col gap-1 p-0 text-sm", "data-selectable": true, children: d.redirects.map((h, i) => {
                            const t = cellTone("status", h.status);
                            return (_jsxs("li", { className: "flex items-baseline gap-1.5", children: [i > 0 ? _jsx(CornerDownRight, { "aria-hidden": true, className: "size-3 shrink-0 text-fg-3" }) : _jsx("span", { className: "w-3 shrink-0" }), _jsx("span", { className: cn("w-8 shrink-0 tabular", t ? toneText[t] : "text-fg-3"), children: h.status ?? "–" }), _jsx("span", { className: "min-w-0 break-all", children: h.url })] }, `${h.id}-${i}`));
                        }) })] })), _jsx(Section, { title: "Links", aside: _jsx(Segmented, { size: "sm", label: "Link list", value: list, onChange: setList, options: [
                        { value: "inlinks", label: `In ${d.inlinks.toLocaleString()}` },
                        { value: "outlinks", label: `Out ${d.outlinks.toLocaleString()}` },
                        { value: "images", label: `Images ${d.images.toLocaleString()}` },
                        ...(d.unavailable.headings ? [] : [{ value: "headings", label: `Headings ${d.headings.toLocaleString()}` }]),
                    ] }), children: _jsx(LinkList, { transport: transport, path: path, id: d.id, kind: list, total: counts[list] }) }), d.headers.length > 0 && (_jsx(Section, { title: `Response headers (${d.headers.length})`, children: _jsx("dl", { className: "m-0 flex flex-col gap-0.5 font-mono text-xs", "data-selectable": true, children: d.headers.map(([k, v], i) => (_jsxs("div", { className: "flex gap-2", children: [_jsx("dt", { className: "shrink-0 text-fg-3", children: k }), _jsx("dd", { className: "m-0 min-w-0 break-all text-fg-2", children: v })] }, `${k}-${i}`))) }) })), missing.size > 0 && (_jsx(Section, { title: "Not collected", children: [...missing].map(([why, fields]) => (_jsxs("p", { className: "m-0 text-sm text-fg-3", children: [_jsxs("span", { className: "text-fg-2", children: [fields.join(", "), "."] }), " ", why] }, why))) }))] }));
}
