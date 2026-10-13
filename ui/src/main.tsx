import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { ErrorBoundary } from "./shell/ErrorBoundary";
import { tauriTransport } from "./transport";
import "./styles/app.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ErrorBoundary>
      <App transport={tauriTransport} />
    </ErrorBoundary>
  </StrictMode>,
);
