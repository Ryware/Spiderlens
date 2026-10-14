import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { useEffect, useState } from "react";
import { FileUp } from "lucide-react";
import { Button, Notice, Textarea } from "../components/ui";
export function ListInputPanel({ transport, summary, onParsed, }) {
    const [text, setText] = useState("");
    const [progress, setProgress] = useState(null);
    const [error, setError] = useState(null);
    useEffect(() => {
        let stop;
        transport.onListProgress((n) => setProgress(n)).then((s) => (stop = s));
        return () => stop?.();
    }, [transport]);
    const run = async (work) => {
        setError(null);
        setProgress(0);
        try {
            onParsed(await work());
        }
        catch (e) {
            setError(String(e));
        }
        finally {
            setProgress(null);
        }
    };
    const upload = async () => {
        const path = await transport.pickListFile();
        if (path)
            await run(() => transport.parseListFile(path));
    };
    return (_jsxs("div", { className: "flex w-full flex-col gap-2", children: [_jsx(Textarea, { "aria-label": "URL list", rows: 6, placeholder: "Paste URLs, one per line", value: text, onChange: (e) => setText(e.target.value) }), _jsxs("div", { className: "flex items-center gap-2", children: [_jsx(Button, { size: "sm", variant: "primary", onClick: () => run(() => transport.parseListText(text)), children: "Load pasted URLs" }), _jsxs(Button, { size: "sm", onClick: upload, children: [_jsx(FileUp, { className: "size-3.5" }), " Upload text or CSV file\u2026"] }), progress !== null && (_jsxs("span", { role: "status", className: "text-sm text-fg-3 tabular", children: ["Reading\u2026 ", progress.toLocaleString(), " lines"] }))] }), error && (_jsx(Notice, { kind: "bad", role: "alert", children: error })), summary && (_jsxs("div", { className: "flex flex-col gap-1.5 rounded-md bg-surface-2 px-3 py-2", "data-testid": "list-summary", children: [_jsxs("p", { className: "m-0 text-sm text-fg-2 tabular", children: [_jsx("strong", { className: "text-fg", children: summary.urls.toLocaleString() }), " URLs to crawl \u00B7 ", summary.duplicates.toLocaleString(), " duplicates removed \u00B7", " ", summary.invalid_count.toLocaleString(), " invalid"] }), summary.notices.map((n) => (_jsx(Notice, { children: n }, n))), summary.invalid_count > 0 && (_jsxs("details", { className: "text-sm", children: [_jsx("summary", { className: "cursor-default text-fg-2", children: "Show invalid lines" }), _jsx("ul", { className: "mt-1 mb-0 pl-4", children: summary.invalid_sample.map((l) => (_jsxs("li", { className: "text-fg-2", children: ["Line ", l.line, ": ", _jsx("code", { className: "font-code text-fg", children: l.text }), " (", l.reason, ")"] }, l.line))) })] }))] }))] }));
}
