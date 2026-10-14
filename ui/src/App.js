import { jsx as _jsx } from "react/jsx-runtime";
import { CSPProvider } from "@base-ui/react/csp-provider";
import { TooltipProvider } from "./components/ui";
import { AppShell } from "./shell/AppShell";
export function App({ transport }) {
    return (
    // Base UI may not inject <style> tags: the CSP stays style-src 'self' (research R2).
    _jsx(CSPProvider, { disableStyleElements: true, children: _jsx(TooltipProvider, { delay: 500, children: _jsx(AppShell, { transport: transport }) }) }));
}
