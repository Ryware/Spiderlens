import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { Menu as BaseMenu } from "@base-ui/react/menu";
import { Check, Columns3 } from "lucide-react";
import { Button, cn } from "../components/ui";
import { needReason } from "./format";
/** Show and hide a tab's columns (FR-014). Order follows the tab unless dragged. */
export function ColumnMenu({ tab, visible, available, onChange, onReset }) {
    const on = new Set(visible);
    const toggle = (id, show) => {
        if (show) {
            // Keep the tab's own order unless the user has dragged columns around.
            const rank = (x) => tab.columns.findIndex((c) => c.id === x);
            const inTabOrder = visible.every((v, i) => i === 0 || rank(visible[i - 1]) < rank(v));
            onChange(inTabOrder ? [...visible, id].sort((a, b) => rank(a) - rank(b)) : [...visible, id]);
        }
        else if (visible.length > 1)
            onChange(visible.filter((v) => v !== id));
    };
    return (_jsxs(BaseMenu.Root, { children: [_jsx(BaseMenu.Trigger, { render: _jsxs(Button, { size: "sm", variant: "quiet", "aria-label": "Columns", children: [_jsx(Columns3, { "aria-hidden": true, className: "size-3.5" }), "Columns"] }) }), _jsx(BaseMenu.Portal, { children: _jsx(BaseMenu.Positioner, { sideOffset: 4, align: "end", className: "z-50", children: _jsxs(BaseMenu.Popup, { className: "max-h-96 min-w-56 overflow-y-auto rounded-lg bg-surface p-1 text-base text-fg shadow-popover outline-none", children: [tab.columns.map((c) => (_jsxs(BaseMenu.CheckboxItem, { checked: on.has(c.id), closeOnClick: false, onCheckedChange: (v) => toggle(c.id, v), className: "flex h-7 items-center gap-2 rounded-sm px-2 outline-none select-none data-highlighted:bg-accent data-highlighted:text-accent-fg", children: [_jsx("span", { className: "flex size-3.5 items-center justify-center", children: _jsx(BaseMenu.CheckboxItemIndicator, { children: _jsx(Check, { "aria-hidden": true, className: "size-3.5" }) }) }), _jsx("span", { className: "flex-1", children: c.label }), !available(c.need) && (_jsx("span", { title: needReason[c.need], className: cn("text-xs opacity-70"), children: "Not collected" }))] }, c.id))), _jsx(BaseMenu.Separator, { className: "mx-1 my-1 h-px bg-line" }), _jsx(BaseMenu.Item, { onClick: onReset, className: "flex h-7 items-center rounded-sm px-2 pl-7.5 outline-none select-none data-highlighted:bg-accent data-highlighted:text-accent-fg", children: "Reset columns" })] }) }) })] }));
}
