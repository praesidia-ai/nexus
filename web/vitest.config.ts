import { defineConfig } from "vitest/config";
import path from "node:path";

// Vitest config for Nexus frontend.
// - jsdom so we can mount React components without a browser.
// - Exclude Playwright's E2E tree so both runners can coexist.
export default defineConfig({
  test: {
    environment: "jsdom",
    globals: true,
    include: ["src/**/*.{test,spec}.{ts,tsx}"],
    exclude: ["node_modules", ".next", "tests/e2e/**"],
    setupFiles: ["./vitest.setup.ts"],
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
});
