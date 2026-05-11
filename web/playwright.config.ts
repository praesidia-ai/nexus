import { defineConfig, devices } from "@playwright/test";

/**
 * Playwright configuration for nexus-web E2E tests.
 *
 * The dev server is started automatically via `npm run dev` on port 3005.
 * API calls are typically mocked at the test level using `page.route(...)`
 * so the suite is fast and does not require a live Rust backend.
 */
export default defineConfig({
  testDir: "./tests/e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: process.env.CI ? "github" : "html",
  timeout: 30_000,
  expect: { timeout: 5_000 },
  use: {
    baseURL: process.env.NEXUS_E2E_BASE_URL ?? "http://localhost:3005",
    trace: "on-first-retry",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: {
    command: "npm run dev",
    url: "http://localhost:3005",
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
    stdout: "ignore",
    stderr: "pipe",
  },
});
