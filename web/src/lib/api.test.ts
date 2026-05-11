import { describe, expect, it, beforeEach, vi, afterEach } from "vitest";

// We only want to exercise the `ApiError` class + `parseError` logic; that
// lives in api.ts and is used by every fetch path. Keep this test hermetic —
// no network.

describe("parseError", () => {
  beforeEach(() => {
    // Avoid noisy console.error on expected paths.
    vi.spyOn(console, "error").mockImplementation(() => {});
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  async function runParse(bodyJson: unknown, status = 400): Promise<string> {
    const body = typeof bodyJson === "string" ? bodyJson : JSON.stringify(bodyJson);
    const res = new Response(body, { status });
    // Import inside each test so module state is fresh.
    const mod = await import("./api");
    // @ts-expect-error — parseError isn't exported, but we re-exercise via fetch path.
    // Fallback: call our own minimal replica if not exported.
    const parseError = (mod as unknown as { parseError?: (r: Response) => Promise<Error> })
      .parseError;
    if (parseError) {
      const err = await parseError(res);
      return err.message;
    }
    // If not exported, simulate the behaviour via the public fetch helper
    // by stubbing global fetch.
    const originalFetch = globalThis.fetch;
    globalThis.fetch = vi.fn(async () => res) as unknown as typeof fetch;
    try {
      await mod.api.listProjects();
      return "(no-error)";
    } catch (e) {
      return (e as Error).message;
    } finally {
      globalThis.fetch = originalFetch;
    }
  }

  it("extracts backend envelope { error: { message } }", async () => {
    const msg = await runParse({ error: { code: "NOT_FOUND", message: "project not found" } }, 404);
    expect(msg).toContain("project not found");
  });

  it("appends hint when present", async () => {
    const msg = await runParse(
      { error: { code: "BAD_REQUEST", message: "description is required", hint: "Send a non-empty prompt" } },
      400,
    );
    expect(msg).toContain("description is required");
    expect(msg).toContain("Send a non-empty prompt");
  });

  it("handles legacy string-shaped error", async () => {
    const msg = await runParse({ error: "legacy string" }, 500);
    expect(msg).toContain("legacy string");
  });

  it("never surfaces [object Object] even with unexpected shape", async () => {
    const msg = await runParse({ error: { something: "weird" } }, 500);
    expect(msg).not.toContain("[object Object]");
  });
});
