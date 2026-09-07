import { describe, expect, it } from "vitest";
import { PRESET_DAYS } from "./periodPresets";
import { DEFAULT_PERIOD_DAYS, decodeScenario, encodeScenario } from "./scenarioUrl";

const state = {
  levers: [
    { nodeId: "inventory.days_in_stock", raw: "11" },
    { nodeId: "orders.unit_price", raw: "+5%" }
  ],
  periodDays: 90,
  timeDimension: "orders.order_date",
  instance: { entity: "supplier", key: "acme" }
};

describe("scenarioUrl", () => {
  it("round-trips a full scenario", () => {
    expect(decodeScenario(encodeScenario(state))).toEqual(state);
  });

  it("round-trips a scenario with no scope", () => {
    const scoped = { ...state, instance: null };
    expect(decodeScenario(encodeScenario(scoped))).toEqual(scoped);
  });

  it("survives a value containing a colon or a percent sign", () => {
    const odd = {
      ...state,
      levers: [{ nodeId: "orders.rate", raw: "+5%" }],
      instance: { entity: "supplier", key: '["a:b","c"]' }
    };
    expect(decodeScenario(encodeScenario(odd))).toEqual(odd);
  });

  it("degrades a malformed lever to no lever rather than throwing", () => {
    const params = new URLSearchParams("lever=garbage&period=90d");
    expect(decodeScenario(params).levers).toEqual([]);
  });

  it("degrades a malformed period to the default", () => {
    const params = new URLSearchParams("period=notaperiod");
    expect(decodeScenario(params).periodDays).toBe(90);
  });

  it("returns an empty scenario for empty params", () => {
    expect(decodeScenario(new URLSearchParams())).toEqual({
      levers: [],
      periodDays: 90,
      timeDimension: null,
      instance: null
    });
  });

  it("defaults to a period the toolbar actually offers", () => {
    expect(PRESET_DAYS).toContain(DEFAULT_PERIOD_DAYS);
  });
});
