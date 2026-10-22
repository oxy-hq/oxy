// @vitest-environment jsdom

import { describe, expect, it } from "vitest";
import { invocationCallsPollMs } from "./useAppFunctions";

describe("invocationCallsPollMs", () => {
  // The server bounds `running` (the function ceiling plus grace, and 15 s past
  // a settle), so the client follows it for as long as it says so.
  it("polls while the server says the trace is still running", () => {
    expect(invocationCallsPollMs("running")).toBe(2000);
  });

  it("never polls a trace that is recorded or will never be", () => {
    for (const reason of [null, "expired", "not_recorded", "did_not_start"]) {
      expect(invocationCallsPollMs(reason)).toBe(false);
    }
  });
});
