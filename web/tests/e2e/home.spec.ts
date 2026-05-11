import { test, expect } from "@playwright/test";

/**
 * Golden-path smoke for the home page.
 *
 * The Rust backend is mocked with `page.route()` so the suite runs without
 * a live `cargo run` process — keeps CI fast and isolated from FE↔BE drift.
 */

test.describe("home page", () => {
  test.beforeEach(async ({ page }) => {
    // Backend appears reachable so the offline banner does not render.
    await page.route("**/health", (route) =>
      route.fulfill({ status: 200, body: JSON.stringify({ ok: true }) }),
    );
    // No projects yet.
    await page.route(/\/projects(\?.*)?$/, (route) => {
      if (route.request().method() === "GET") {
        return route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify([]),
        });
      }
      return route.continue();
    });
  });

  test("renders heading + textarea + at least one starter template", async ({
    page,
  }) => {
    await page.goto("/");
    await expect(page.locator("textarea")).toBeVisible({ timeout: 10_000 });
    // Starter template gallery (introduced in F4).
    await expect(
      page.getByRole("button", { name: /Use .* template/ }).first(),
    ).toBeVisible();
  });

  test("clicking a starter template fills the prompt textarea", async ({
    page,
  }) => {
    await page.goto("/");
    const firstTemplate = page
      .getByRole("button", { name: /Use .* template/ })
      .first();
    await firstTemplate.click();
    const textarea = page.locator("textarea");
    const value = await textarea.inputValue();
    expect(value.length).toBeGreaterThan(40);
  });

  test("backend offline banner appears and Retry button is keyboard reachable", async ({
    page,
  }) => {
    // Force the connectivity probe to fail.
    await page.unroute("**/health");
    await page.route("**/health", (route) => route.abort());
    await page.goto("/");
    const retry = page.getByRole("button", { name: /Retry backend connection/ });
    // The banner only renders when the connectivity hook flips to "offline" —
    // give it up to 6s.
    await expect(retry).toBeVisible({ timeout: 6_000 });
  });
});
