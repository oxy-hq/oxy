import { describe, expect, it } from "vitest";
import type { ScenarioState } from "./scenarioUrl";
import { buildParams } from "./urlParams";

const scenario: ScenarioState = {
  levers: [{ nodeId: "orders.unit_price", raw: "+5%" }],
  periodDays: 90,
  timeDimension: "orders.order_date",
  instance: null
};

describe("buildParams", () => {
  it("preserves unrelated params (e.g. the IDE tab's `view`) while writing scenario keys", () => {
    const base = new URLSearchParams("view=metric-tree&foo=bar");
    const result = buildParams(base, "scenario", scenario);

    expect(result.get("view")).toBe("metric-tree");
    expect(result.get("foo")).toBe("bar");
    expect(result.get("mode")).toBe("scenario");
    expect(result.get("period")).toBe("90d");
    expect(result.get("time_dim")).toBe("orders.order_date");
    expect(result.getAll("lever")).toEqual([JSON.stringify(["orders.unit_price", "+5%"])]);
  });

  it("drops stale `lever` entries when the new scenario has fewer levers than the base", () => {
    const base = new URLSearchParams(
      "view=metric-tree&lever=" +
        encodeURIComponent(JSON.stringify(["a", "1"])) +
        "&lever=" +
        encodeURIComponent(JSON.stringify(["b", "2"]))
    );
    const oneLever: ScenarioState = { ...scenario, levers: [{ nodeId: "a", raw: "1" }] };

    const result = buildParams(base, "explore", oneLever);

    expect(result.getAll("lever")).toEqual([JSON.stringify(["a", "1"])]);
    expect(result.get("view")).toBe("metric-tree");
    // "explore" mode must not leave a stale `mode=scenario` behind either.
    expect(result.has("mode")).toBe(false);
  });

  it("clears mode entirely for explore even if the base had mode=scenario", () => {
    const base = new URLSearchParams("view=metric-tree&mode=scenario");
    const result = buildParams(base, "explore", { ...scenario, levers: [] });

    expect(result.has("mode")).toBe(false);
    expect(result.get("view")).toBe("metric-tree");
  });
});
