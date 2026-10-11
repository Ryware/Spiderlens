// The only module that knows how the UI talks to the engine. The desktop app
// uses Tauri commands; a future web version replaces this file with HTTP calls
// and leaves every component untouched (constitution principle III).

import { invoke } from "@tauri-apps/api/core";

export interface BuildInfo {
  product: string;
  product_version: string;
  engine_version: string;
  build_id: string;
  commit: string | null;
}

export interface Transport {
  buildInfo(): Promise<BuildInfo>;
}

export const tauriTransport: Transport = {
  buildInfo: () => invoke<BuildInfo>("build_info"),
};
