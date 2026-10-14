import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { Plus, X } from "lucide-react";
import { Button, Input, Switch } from "../components/ui";
export function HeadersEditor({ headers, onChange }) {
    const update = (i, h) => onChange(headers.map((x, j) => (j === i ? h : x)));
    return (_jsxs("div", { className: "flex w-full flex-col gap-2", children: [headers.map((h, i) => {
                const secret = "secret" in h.value;
                const value = "plain" in h.value ? h.value.plain : (h.value.value ?? "");
                return (_jsxs("div", { className: "flex flex-wrap items-center gap-2", children: [_jsx(Input, { "aria-label": `Header ${i + 1} name`, placeholder: "Name", mono: true, className: "w-40", value: h.name, onChange: (e) => update(i, { ...h, name: e.target.value }) }), _jsx(Input, { "aria-label": `Header ${i + 1} value`, placeholder: secret && !value ? "Re-enter secret value" : "Value", type: secret ? "password" : "text", mono: true, className: "min-w-0 flex-1", value: value, onChange: (e) => update(i, { ...h, value: secret ? { secret: true, value: e.target.value } : { plain: e.target.value } }) }), _jsxs("span", { className: "flex items-center gap-1.5 text-sm text-fg-2", children: [_jsx(Switch, { label: `Header ${i + 1} secret`, checked: secret, onChange: (s) => update(i, { ...h, value: s ? { secret: true, value } : { plain: value } }) }), "Secret"] }), _jsxs("span", { className: "flex items-center gap-1.5 text-sm text-fg-2", children: [_jsx(Switch, { label: `Header ${i + 1} all hosts`, checked: h.send_to === "all", onChange: (all) => update(i, { ...h, send_to: all ? "all" : "internal" }) }), "All hosts"] }), _jsx(Button, { variant: "quiet", icon: true, size: "sm", "aria-label": `Remove header ${i + 1}`, onClick: () => onChange(headers.filter((_, j) => j !== i)), children: _jsx(X, { className: "size-3.5" }) })] }, i));
            }), _jsx("div", { children: _jsxs(Button, { size: "sm", onClick: () => onChange([...headers, { name: "", value: { plain: "" }, send_to: "internal" }]), children: [_jsx(Plus, { className: "size-3.5" }), " Add header"] }) })] }));
}
