import { jsx as _jsx, jsxs as _jsxs, Fragment as _Fragment } from "react/jsx-runtime";
import { forwardRef } from "react";
import { Command as CommandIcon, ListPlus, Pause, Play, RotateCcw, SlidersHorizontal, Square, TriangleAlert } from "lucide-react";
import { Button, cn, Input, Kbd, Segmented, Tooltip } from "../components/ui";
import { platform } from "./platform";
import { BrightnessPopover } from "./BrightnessPopover";
/**
 * The crawl lives here (FR-002): mode, start URL or list, Start, settings and
 * the palette. Exactly one primary control: Start, or Pause while crawling,
 * or Resume while paused. Mode and URL are locked while a crawl is open.
 * "Retry failed URLs" sits next to them once a crawl file is open.
 */
export const Toolbar = forwardRef(function Toolbar({ setup, run, onStart, onResume, onOpenSettings, onOpenPalette, appearance, level, systemDark, onAppearance, retry, onRetry }, urlRef) {
    const { config, list } = setup;
    const locked = run.active;
    const mode = config?.mode ?? "spider";
    const firstError = setup.errors[0]?.message;
    const urlError = setup.errors.find((e) => e.path === "input.start_url");
    const warnings = setup.warnings.length;
    return (_jsxs("header", { "data-tauri-drag-region": true, "data-region": "chrome", "data-material-region": true, className: cn("flex h-toolbar shrink-0 items-center gap-2 border-b border-line bg-chrome px-3", platform === "mac" && "pl-mac-inset"), children: [_jsx(Segmented, { label: "Crawl mode", value: mode, options: [
                    { value: "spider", label: "Spider", disabled: locked },
                    { value: "list", label: "List", disabled: locked },
                ], onChange: (m) => setup.set((c) => ({ ...c, mode: m })) }), mode === "spider" ? (_jsx(Input, { ref: urlRef, "aria-label": "Start URL", "aria-invalid": setup.startUrl !== "" && !!urlError, 
                // A native title, not <Tooltip>: wrapping the field when an error
                // appears would remount it and drop focus while typing.
                title: setup.startUrl !== "" ? urlError?.message : undefined, placeholder: "Enter a website, for example example.com", mono: true, spellCheck: false, autoCapitalize: "off", autoCorrect: "off", className: "h-control min-w-0 flex-1 bg-surface-2", readOnly: locked, value: setup.startUrl, onChange: (e) => setup.setStartUrl(e.target.value), onKeyDown: (e) => e.key === "Enter" && !locked && onStart() })) : (_jsxs("button", { type: "button", disabled: locked, onClick: () => onOpenSettings("list"), className: "flex h-control min-w-0 flex-1 items-center gap-2 rounded-md bg-surface-2 px-2 text-left text-base text-fg-2 hover:bg-surface-3", children: [_jsx(ListPlus, { "aria-hidden": true, className: "size-4 text-fg-3" }), _jsx("span", { className: "truncate tabular", children: list ? `${list.urls.toLocaleString()} URLs loaded` : "Add a list of URLs…" })] })), warnings > 0 && (_jsx(Tooltip, { content: setup.warnings.map((w) => w.message).join(" "), children: _jsxs(Button, { variant: "quiet", onClick: () => onOpenSettings(), "aria-label": `${warnings} less respectful settings`, className: "text-warn", children: [_jsx(TriangleAlert, { className: "size-4" }), _jsx("span", { className: "tabular", children: warnings })] }) })), run.path && (_jsx(Tooltip, { content: retry.available ? null : retry.reason, children: _jsx("span", { className: "inline-flex", children: _jsxs(Button, { variant: "quiet", disabled: !retry.available, onClick: onRetry, "aria-label": "Retry failed URLs", children: [_jsx(RotateCcw, { "aria-hidden": true, className: "size-3.5" }), " Retry failed", retry.available && _jsx("span", { className: "text-fg-3 tabular", children: retry.total.toLocaleString() })] }) }) })), _jsx(CrawlControls, { setup: setup, run: run, onStart: onStart, onResume: onResume, firstError: firstError }), _jsxs(Button, { onClick: () => onOpenSettings(), "aria-label": "Configure", children: [_jsx(SlidersHorizontal, { "aria-hidden": true, className: "size-3.5" }), " Configure ", _jsx(Kbd, { keys: "Mod+," })] }), _jsx(BrightnessPopover, { appearance: appearance, level: level, systemDark: systemDark, onChange: onAppearance }), _jsx(Tooltip, { content: "Commands", children: _jsx(Button, { variant: "quiet", icon: true, onClick: onOpenPalette, "aria-label": "Open command palette", children: _jsx(CommandIcon, { className: "size-4" }) }) })] }));
});
function CrawlControls({ setup, run, onStart, onResume, firstError }) {
    const { phase } = run;
    if (phase === "running" || phase === "starting" || phase === "pausing" || phase === "stopping") {
        const settling = phase !== "running";
        return (_jsxs(_Fragment, { children: [_jsxs(Button, { variant: "primary", disabled: settling, onClick: run.pause, children: [_jsx(Pause, { "aria-hidden": true, className: "size-3.5 fill-current" }), " ", phase === "pausing" ? "Pausing…" : "Pause"] }), _jsxs(Button, { disabled: settling, onClick: run.stop, children: [_jsx(Square, { "aria-hidden": true, className: "size-3 fill-current" }), " ", phase === "stopping" ? "Stopping…" : "Stop"] })] }));
    }
    if (phase === "paused") {
        return (_jsxs(_Fragment, { children: [_jsxs(Button, { variant: "primary", onClick: onResume, children: [_jsx(Play, { "aria-hidden": true, className: "size-3.5 fill-current" }), " Resume"] }), _jsxs(Button, { onClick: run.stop, children: [_jsx(Square, { "aria-hidden": true, className: "size-3 fill-current" }), " Stop"] })] }));
    }
    return (_jsx(Tooltip, { content: !setup.canStart && firstError ? firstError : null, children: _jsx("span", { className: "inline-flex", children: _jsxs(Button, { variant: "primary", disabled: !setup.canStart, onClick: onStart, children: [_jsx(Play, { "aria-hidden": true, className: "size-3.5 fill-current" }), " Start"] }) }) }));
}
