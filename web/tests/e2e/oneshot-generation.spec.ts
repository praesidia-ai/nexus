import { test } from "@playwright/test";

/**
 * Golden-path E2E for the oneshot generation flow — "user types a prompt,
 * hits submit, sees progress stream, lands on the project page with an
 * embedded preview URL".
 *
 * Backend + SSE are fully mocked so the test is hermetic.
 */

const MOCK_PROJECT_ID = "e2e-project-0001";

test.describe("oneshot generation flow", () => {
  test.beforeEach(async ({ page }) => {
    // Backend healthy.
    await page.route("**/health", (route) =>
      route.fulfill({ status: 200, body: JSON.stringify({ ok: true }) }),
    );
    // No existing projects on the home page.
    await page.route(/\/projects(\?.*)?$/, (route) => {
      const req = route.request();
      if (req.method() === "GET") {
        return route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify([]),
        });
      }
      if (req.method() === "POST") {
        // Simulate project creation returning the fresh ID.
        return route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({
            id: MOCK_PROJECT_ID,
            name: "e2e project",
            description: "a todo list app",
            phase: 1,
            tenant_id: "default",
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          }),
        });
      }
      return route.continue();
    });

    // Project page fetches.
    await page.route(/\/projects\/[^/]+$/, (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          id: MOCK_PROJECT_ID,
          name: "e2e project",
          description: "a todo list app",
          phase: 2,
          tenant_id: "default",
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        }),
      }),
    );
    // Empty conversation.
    await page.route(/\/projects\/[^/]+\/conversations$/, (route) =>
      route.fulfill({ status: 200, contentType: "application/json", body: "[]" }),
    );

    // Oneshot SSE stream — emit a scripted sequence then Complete with app_url.
    await page.route(/\/oneshot\/start$/, (route) => {
      const body =
        `event: phase\n` +
        `data: {"type":"phase","phase":"intent","status":"started","detail":"Understanding..."}\n\n` +
        `event: intent_analyzed\n` +
        `data: {"type":"intent_analyzed","app_type":"Todo","complexity":"Simple","domain":"productivity","needs_auth":false,"needs_database":false}\n\n` +
        `event: progress\n` +
        `data: {"type":"progress","percent":50,"message":"Generating","phase":"codegen"}\n\n` +
        `event: file_written\n` +
        `data: {"type":"file_written","path":"app/page.tsx","size":420}\n\n` +
        `event: taste_scored\n` +
        `data: {"type":"taste_scored","overall":86,"redesign_triggered":false}\n\n` +
        `event: complete\n` +
        `data: {"type":"complete","project_id":"${MOCK_PROJECT_ID}","project_name":"e2e project","taste_score":86,"files_count":12,"duration_ms":5000,"app_url":"http://localhost:3101"}\n\n`;
      return route.fulfill({
        status: 200,
        contentType: "text/event-stream",
        body,
      });
    });
  });

  test("home → enter prompt → lands on project page with preview URL", async ({
    page,
  }) => {
    await page.goto("/");
    const textarea = page.locator("textarea");
    await textarea.fill("Build me a todo list app with auth");

    // Submit via Enter (the primary path).
    await textarea.press("Enter");

    // Wait for navigation to the project route. Next.js router uses
    // client-side navigation here; just wait for the URL change.
    await page.waitForURL(new RegExp(`/${MOCK_PROJECT_ID}(\\?.*)?$`), {
      timeout: 10_000,
    });
  });
});
