import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { useState } from "react";
import { Badge, Button, Textarea } from "../components/ui";
export function PatternTester({ transport, include, exclude }) {
    const [samples, setSamples] = useState("");
    const [rows, setRows] = useState([]);
    const run = async () => setRows(await transport.testPatterns(include, exclude, samples.split("\n")));
    return (_jsxs("div", { className: "flex w-full flex-col gap-2", children: [_jsx(Textarea, { "aria-label": "Sample URLs", rows: 3, placeholder: "Paste sample URLs, one per line", value: samples, onChange: (e) => setSamples(e.target.value) }), _jsx("div", { children: _jsx(Button, { size: "sm", onClick: run, children: "Test patterns" }) }), rows.length > 0 && (_jsxs("table", { className: "w-full border-collapse text-sm", children: [_jsx("thead", { children: _jsxs("tr", { className: "text-left text-xs text-fg-3", children: [_jsx("th", { className: "py-1 pr-2 font-medium", children: "URL" }), _jsx("th", { className: "py-1 pr-2 font-medium", children: "Result" }), _jsx("th", { className: "py-1 font-medium", children: "Decided by" })] }) }), _jsx("tbody", { children: rows.map((r, i) => (_jsxs("tr", { className: "border-t border-line", children: [_jsx("td", { className: "max-w-60 truncate py-1 pr-2 font-code", "data-selectable": true, children: r.url ?? r.input }), _jsx("td", { className: "py-1 pr-2", children: r.error ? _jsx(Badge, { tone: "bad", children: r.error }) : describe(r) }), _jsx("td", { className: "py-1 font-code text-fg-2", children: r.pattern ?? "" })] }, i))) })] }))] }));
}
function describe(r) {
    switch (r.decision?.decision) {
        case "crawl":
            return _jsx(Badge, { tone: "ok", children: "Crawled" });
        case "excluded":
            return _jsx(Badge, { tone: "warn", children: "Excluded" });
        case "not_included":
            return _jsx(Badge, { tone: "neutral", children: "Not included (matches no include pattern)" });
        default:
            return null;
    }
}
