// End-to-end tests: the real UI in Chromium over the in-memory transport
// (e2e/app.tsx), served by Vite. Component behaviour is tested with Vitest;
// these cover what jsdom cannot: layout, scrolling, real keys and pointers,
// and the theme's computed colors.
import { defineConfig, devices } from "@playwright/test";

const port = 5199;

export default defineConfig({
  testDir: "e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: `http://localhost:${port}/e2e/`,
    trace: "retain-on-failure",
    viewport: { width: 1400, height: 860 },
  },
  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        viewport: { width: 1400, height: 860 },
        // Cloud containers ship their own Chromium; CI installs Playwright's.
        launchOptions: process.env.PLAYWRIGHT_CHROMIUM ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM } : {},
      },
    },
  ],
  webServer: {
    command: `pnpm exec vite --port ${port} --strictPort`,
    url: `http://localhost:${port}/e2e/`,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
