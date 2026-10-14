import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
// The last line of defence: a render error shows what broke instead of an
// empty window, and the window can be reloaded from here.
import { Component } from "react";
import { Button } from "../components/ui";
export class ErrorBoundary extends Component {
    constructor() {
        super(...arguments);
        this.state = { error: null };
    }
    static getDerivedStateFromError(error) {
        return { error };
    }
    componentDidCatch(error) {
        console.error(error);
    }
    render() {
        const { error } = this.state;
        if (!error)
            return this.props.children;
        return (_jsxs("div", { role: "alert", className: "flex h-screen flex-col items-center justify-center gap-3 bg-surface p-6 text-center", children: [_jsx("p", { className: "text-base font-medium text-fg", children: "SpiderLens hit an error and stopped drawing this window." }), _jsx("pre", { className: "max-w-xl whitespace-pre-wrap text-sm text-fg-2", children: error.message || String(error) }), _jsx(Button, { onClick: () => window.location.reload(), children: "Reload window" })] }));
    }
}
