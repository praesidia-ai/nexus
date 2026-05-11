import { describe, it, expect } from "vitest";

// Shape-level contract test for OneshotGenerationState. Pins the fields the
// rest of the UI depends on (notably `appUrl`, which the Complete event
// populates from the backend's app_url field).

import type { OneshotGenerationState } from "./use-oneshot-generation";

describe("OneshotGenerationState", () => {
  it("exposes the appUrl field for bolt-parity preview UX", () => {
    // Build a zero-valued instance via type assertion — we're testing the
    // compile-time shape, not runtime behaviour here.
    const state: OneshotGenerationState = {
      active: false,
      isComplete: false,
      isError: false,
      progress: 0,
      steps: [],
      description: null,
      completedProjectId: null,
      tasteScore: null,
      appUrl: null,
      startedAt: null,
      lastEventAt: null,
      abort: () => {},
      aborted: false,
    };
    expect(state.appUrl).toBeNull();
    expect(typeof state.abort).toBe("function");
  });
});
