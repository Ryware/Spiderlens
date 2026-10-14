import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { forwardRef } from "react";
import { ChevronsUpDown } from "lucide-react";
import { Switch as BaseSwitch } from "@base-ui/react/switch";
import { cn } from "./cn";
const control = "rounded-md border border-control-border bg-surface text-fg placeholder:text-fg-3 " +
    "transition-colors duration-(--dur-fast) hover:enabled:border-fg-3 " +
    "focus-visible:outline-2 focus-visible:outline-accent focus-visible:-outline-offset-1 " +
    "disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-bad";
export const Input = forwardRef(function Input({ className, mono, ...rest }, ref) {
    return (_jsx("input", { ref: ref, className: cn(control, "h-control px-2 text-base", mono && "font-code text-sm", className), ...rest }));
});
export const Textarea = forwardRef(function Textarea({ className, ...rest }, ref) {
    return (_jsx("textarea", { ref: ref, className: cn(control, "min-h-16 w-full resize-y px-2 py-1.5 font-code text-sm leading-normal", className), ...rest }));
});
/**
 * Native select, restyled. The open menu is the platform's own (an NSMenu on
 * macOS), which is the most native-feeling option a webview offers.
 */
export const Select = forwardRef(function Select({ className, children, ...rest }, ref) {
    return (_jsxs("span", { className: cn("relative inline-flex", className), children: [_jsx("select", { ref: ref, className: cn(control, "h-control w-full appearance-none pr-7 pl-2 text-base"), ...rest, children: children }), _jsx(ChevronsUpDown, { "aria-hidden": true, className: "pointer-events-none absolute top-1/2 right-2 size-3.5 -translate-y-1/2 text-fg-3" })] }));
});
/** Number input where blank means `null` when `optional` is set. */
export function NumberInput({ value, onChange, optional, className, ...rest }) {
    return (_jsx(Input, { type: "number", inputMode: "decimal", className: cn("w-24 text-right tabular", className), value: value === null || Number.isNaN(value) ? "" : value, onChange: (e) => onChange(e.target.value === "" ? (optional ? null : NaN) : Number(e.target.value)), ...rest }));
}
/** On/off setting. Name it with `label`, or with `labelledBy` when a visible label exists. */
export function Switch({ checked, onChange, label, labelledBy, disabled, }) {
    return (_jsx(BaseSwitch.Root, { "aria-label": labelledBy ? undefined : label, "aria-labelledby": labelledBy, checked: checked, disabled: disabled, onCheckedChange: (v) => onChange(v), className: cn("relative inline-flex h-4.5 w-8 shrink-0 items-center rounded-full border border-control-border bg-surface-3 p-px", "transition-colors duration-(--dur) data-checked:border-accent data-checked:bg-accent", 
        // :disabled covers a switch inside a disabled fieldset (settings locked while crawling).
        "data-disabled:cursor-not-allowed data-disabled:opacity-50 disabled:cursor-not-allowed disabled:opacity-50"), children: _jsx(BaseSwitch.Thumb, { className: "size-3.5 rounded-full bg-surface shadow-raised transition-transform duration-(--dur) ease-ui data-checked:translate-x-3.5" }) }));
}
/** Single choice among a few options (radio group semantics, arrow keys move). */
export function Segmented({ value, options, onChange, label, size = "md", }) {
    const move = (dir) => {
        const enabled = options.filter((o) => !o.disabled);
        const i = enabled.findIndex((o) => o.value === value);
        const next = enabled[(i + dir + enabled.length) % enabled.length];
        if (next)
            onChange(next.value);
    };
    return (_jsx("div", { role: "radiogroup", "aria-label": label, className: cn("inline-flex shrink-0 gap-0.5 rounded-md bg-surface-2 p-0.5", size === "md" ? "h-control" : "h-6"), onKeyDown: (e) => {
            if (e.key === "ArrowRight" || e.key === "ArrowDown")
                (e.preventDefault(), move(1));
            if (e.key === "ArrowLeft" || e.key === "ArrowUp")
                (e.preventDefault(), move(-1));
        }, children: options.map((o) => {
            const on = o.value === value;
            return (_jsx("button", { type: "button", role: "radio", "aria-checked": on, "aria-label": o.label, disabled: o.disabled, tabIndex: on ? 0 : -1, onClick: () => onChange(o.value), className: cn("rounded-sm px-2.5 text-sm transition-colors duration-(--dur-fast) disabled:opacity-45", on ? "bg-surface text-fg shadow-raised" : "text-fg-2 hover:enabled:text-fg"), children: o.label }, o.value));
        }) }));
}
