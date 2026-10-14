import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { Dialog } from "@base-ui/react/dialog";
import { Menu as BaseMenu } from "@base-ui/react/menu";
import { Popover as BasePopover } from "@base-ui/react/popover";
import { Tooltip as BaseTooltip } from "@base-ui/react/tooltip";
import { cn } from "./cn";
export const TooltipProvider = BaseTooltip.Provider;
/** Tooltip on hover and focus. `content` null renders the child alone. */
export function Tooltip({ content, children }) {
    if (content == null || content === "")
        return children;
    return (_jsxs(BaseTooltip.Root, { children: [_jsx(BaseTooltip.Trigger, { render: children }), _jsx(BaseTooltip.Portal, { children: _jsx(BaseTooltip.Positioner, { sideOffset: 6, className: "z-50", children: _jsx(BaseTooltip.Popup, { className: cn("max-w-80 rounded-md bg-fg px-2 py-1 text-sm text-bg shadow-popover", "origin-(--transform-origin) transition-[opacity,scale] duration-(--dur-fast)", "data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0"), children: content }) }) })] }));
}
/** Modal sheet over the window (FR-009). Escape and the scrim close it. */
export function Sheet({ open, onOpenChange, title, children, className, initialFocus, }) {
    return (_jsx(Dialog.Root, { open: open, onOpenChange: (o) => onOpenChange(o), children: _jsxs(Dialog.Portal, { children: [_jsx(Dialog.Backdrop, { className: "fixed inset-0 z-40 bg-scrim transition-opacity duration-(--dur) data-ending-style:opacity-0 data-starting-style:opacity-0" }), _jsxs(Dialog.Popup, { initialFocus: initialFocus, className: cn("fixed top-12 left-1/2 z-40 flex sheet-max-h -translate-x-1/2 flex-col overflow-hidden", "rounded-xl bg-surface text-fg shadow-sheet outline-none", "transition-[opacity,translate] duration-(--dur-slow) ease-ui", "data-ending-style:-translate-y-3 data-ending-style:opacity-0 data-starting-style:-translate-y-3 data-starting-style:opacity-0", className), children: [_jsx(Dialog.Title, { className: "sr-only", children: title }), children] })] }) }));
}
export const SheetClose = Dialog.Close;
/** Action menu. `null` entries draw separators. */
export function Menu({ trigger, items, label }) {
    return (_jsxs(BaseMenu.Root, { children: [_jsx(BaseMenu.Trigger, { render: trigger, "aria-label": label }), _jsx(BaseMenu.Portal, { children: _jsx(BaseMenu.Positioner, { sideOffset: 4, align: "start", className: "z-50", children: _jsx(BaseMenu.Popup, { className: cn("min-w-48 rounded-lg bg-surface p-1 text-base text-fg shadow-popover outline-none", "origin-(--transform-origin) transition-[opacity,scale] duration-(--dur-fast)", "data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0"), children: items.map((it, i) => it === null ? (_jsx(BaseMenu.Separator, { className: "mx-1 my-1 h-px bg-line" }, `sep-${i}`)) : (_jsx(BaseMenu.Item, { disabled: it.disabled, onClick: it.onSelect, className: cn("flex h-7 items-center rounded-sm px-2 outline-none select-none", "data-highlighted:bg-accent data-highlighted:text-accent-fg data-disabled:opacity-45", it.destructive && "text-bad"), children: it.label }, it.label))) }) }) })] }));
}
/** Non-modal panel anchored to its trigger, for details behind a compact control (paused sites). */
export function Popover({ trigger, title, children, side = "bottom", align = "start", className, }) {
    return (_jsxs(BasePopover.Root, { children: [_jsx(BasePopover.Trigger, { render: trigger }), _jsx(BasePopover.Portal, { children: _jsx(BasePopover.Positioner, { side: side, align: align, sideOffset: 6, className: "z-50", children: _jsxs(BasePopover.Popup, { className: cn("flex max-w-110 min-w-72 flex-col rounded-lg bg-surface text-fg shadow-popover outline-none", "origin-(--transform-origin) transition-[opacity,scale] duration-(--dur-fast)", "data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0", className), children: [_jsx(BasePopover.Title, { className: "m-0 border-b border-line px-3 py-2 text-xs font-semibold text-fg-3", children: title }), children] }) }) })] }));
}
/** Small modal for one question or one input (profile names, About). */
export function Dialog_({ open, onOpenChange, title, description, children, }) {
    return (_jsx(Dialog.Root, { open: open, onOpenChange: (o) => onOpenChange(o), children: _jsxs(Dialog.Portal, { children: [_jsx(Dialog.Backdrop, { className: "fixed inset-0 z-40 bg-scrim transition-opacity duration-(--dur) data-ending-style:opacity-0 data-starting-style:opacity-0" }), _jsxs(Dialog.Popup, { className: cn("fixed top-1/3 left-1/2 z-40 flex w-96 -translate-x-1/2 -translate-y-1/2 flex-col gap-3 rounded-xl bg-surface p-5 text-fg shadow-sheet outline-none", "transition-[opacity,scale] duration-(--dur) data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0"), children: [_jsxs("div", { className: "flex flex-col gap-1", children: [_jsx(Dialog.Title, { className: "m-0 text-md font-semibold", children: title }), description && _jsx(Dialog.Description, { className: "m-0 text-sm text-fg-2", children: description })] }), children] })] }) }));
}
export { Dialog_ as Modal };
