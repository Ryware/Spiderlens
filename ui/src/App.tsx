import { CSPProvider } from "@base-ui/react/csp-provider";
import type { Transport } from "./transport";
import { TooltipProvider } from "./components/ui";
import { AppShell } from "./shell/AppShell";

export function App({ transport }: { transport: Transport }) {
  return (
    // Base UI may not inject <style> tags: the CSP stays style-src 'self' (research R2).
    <CSPProvider disableStyleElements>
      <TooltipProvider delay={500}>
        <AppShell transport={transport} />
      </TooltipProvider>
    </CSPProvider>
  );
}
