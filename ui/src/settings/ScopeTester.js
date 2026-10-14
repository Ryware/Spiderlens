import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { useState } from "react";
import { Badge, Button, Notice, Switch, Textarea } from "../components/ui";
/**
 * What the crawler would do with each pasted URL under every scope setting
 * at once (spec 003 FR-016): patterns, subdomains, folder, query parameters,
 * URL length and, when asked, robots.txt. Requests nothing else.
 */
export function ScopeTester({ transport, config, startUrl }) {
    const [samples, setSamples] = useState("");
    const [robots, setRobots] = useState(false);
    const [rows, setRows] = useState([]);
    const [error, setError] = useState(null);
    const [busy, setBusy] = useState(false);
    const run = async () => {
        if (!startUrl)
            return;
        setBusy(true);
        setError(null);
        try {
            setRows(await transport.scopeTest(config, startUrl, samples.split("\n"), robots));
        }
        catch (e) {
            setError(String(e));
        }
        finally {
            setBusy(false);
        }
    };
    if (!startUrl)
        return _jsx("p", { className: "m-0 text-sm text-fg-3", children: "Enter a start URL first: whether a URL is in scope depends on where the crawl starts." });
    return (_jsxs("div", { className: "flex w-full flex-col gap-2", children: [_jsx(Textarea, { "aria-label": "URLs to test", rows: 3, placeholder: "Paste URLs or paths, one per line", value: samples, onChange: (e) => setSamples(e.target.value) }), _jsxs("div", { className: "flex items-center gap-3", children: [_jsx(Button, { size: "sm", onClick: run, disabled: busy || samples.trim() === "", children: busy ? "Testing…" : "Test URLs" }), _jsxs("label", { className: "flex items-center gap-2 text-sm text-fg-2", children: [_jsx(Switch, { label: "Check robots.txt", checked: robots, onChange: setRobots }), "Check robots.txt (one request per site)"] })] }), error && _jsx(Notice, { kind: "bad", children: error }), rows.length > 0 && (_jsxs("table", { className: "w-full border-collapse text-sm", children: [_jsx("thead", { children: _jsxs("tr", { className: "text-left text-xs text-fg-3", children: [_jsx("th", { className: "py-1 pr-2 font-medium", children: "URL" }), _jsx("th", { className: "py-1 font-medium", children: "Would be" })] }) }), _jsx("tbody", { children: rows.map((r, i) => (_jsxs("tr", { className: "border-t border-line", "data-verdict": r.reason ?? "crawl", children: [_jsx("td", { className: "max-w-60 truncate py-1 pr-2 font-code", title: r.url ?? r.input, "data-selectable": true, children: r.url ?? r.input }), _jsx("td", { className: "py-1", children: r.would_crawl ? (_jsx(Badge, { tone: "ok", children: r.robots_checked ? "Crawled" : "Crawled (robots.txt not checked)" })) : (_jsxs("span", { className: "flex flex-wrap items-center gap-1.5", children: [_jsx(Badge, { tone: r.reason === "external" ? "neutral" : "warn", children: r.label }), r.detail && _jsx("span", { className: "font-code text-fg-2", children: r.detail })] })) })] }, i))) })] }))] }));
}
