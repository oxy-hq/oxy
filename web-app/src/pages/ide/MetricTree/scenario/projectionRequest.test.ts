import { describe, expect, it } from "vitest";
import { buildProjectionRequest } from "./projectionRequest";
import type { ScenarioState } from "./scenarioUrl";

const STATE: ScenarioState = {
  levers: [{ nodeId: "store_days.marketing_spend", value: 100 }],
  timeDimension: "store_days.business_date",
  periodDays: 30,
  instance: null
} as unknown as ScenarioState;

describe("buildProjectionRequest", () => {
  it("builds a request for a valid lever set", () => {
    const req = buildProjectionRequest(false, STATE, "day", 30);
    expect(req).not.toBeNull();
    expect(req?.horizon).toBe(30);
  });

  /** A conflicting lever set costs no query, whatever else is asked. */
  it("stays null when blocked", () => {
    expect(buildProjectionRequest(true, STATE, "day", 30)).toBeNull();
  });
});
