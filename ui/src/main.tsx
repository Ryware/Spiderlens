import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { tauriTransport } from "./transport";
import "./styles/app.css";
import { applyStoredAppearance } from "./shell/useUiState";

applyStoredAppearance();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App transport={tauriTransport} />
  </StrictMode>,
);
