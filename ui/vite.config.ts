import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Tauri expects a fixed dev port and does not want the screen cleared.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { target: "es2021", outDir: "dist" },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/testing/setup.ts"],
    // Playwright owns e2e/; Vitest runs the component tests in src/.
    include: ["src/**/*.test.{ts,tsx}"],
    // main.tsx and transport.ts only wire up Tauri; the desktop build checks them.
    coverage: { include: ["src/**"], exclude: ["src/generated/**", "src/testing/**", "src/**/*.test.*", "src/main.tsx", "src/transport.ts"] },
  },
});
