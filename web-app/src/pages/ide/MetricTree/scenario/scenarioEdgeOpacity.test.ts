import { describe, expect, it } from "vitest";
import {
  SCENARIO_NODE_PRESENTATION,
  type ScenarioNodeState,
  scenarioEdgeOpacity
} from "./nodeValue";

const LIT = 0.7;

describe("scenarioEdgeOpacity", () => {
  it("keeps an edge lit only when both endpoints are in the foreground", () => {
    expect(scenarioEdgeOpacity("lever", "impacted", LIT)).toBe(LIT);
    expect(scenarioEdgeOpacity("impacted", "unquantifiable", LIT)).toBe(LIT);
  });

  it("recedes an edge that runs into a dimmed card", () => {
    // The reported bug: a full-strength green edge terminating on a washed-out
    // node, which reads as "the scenario propagated along here".
    expect(scenarioEdgeOpacity("lever", "unchanged", LIT)).toBeLessThan(LIT);
    expect(scenarioEdgeOpacity("impacted", "unreachable", LIT)).toBeLessThan(LIT);
    expect(scenarioEdgeOpacity("unvalued", "impacted", LIT)).toBeLessThan(LIT);
  });

  it("recedes an edge whose endpoint is missing from the scenario map", () => {
    expect(scenarioEdgeOpacity("lever", undefined, LIT)).toBeLessThan(LIT);
    expect(scenarioEdgeOpacity(undefined, undefined, LIT)).toBeLessThan(LIT);
  });

  it("agrees with the card presentation for every state", () => {
    // The two must not drift: whatever dims a card must fade its edges.
    for (const state of Object.keys(SCENARIO_NODE_PRESENTATION) as ScenarioNodeState[]) {
      const isDimmed = SCENARIO_NODE_PRESENTATION[state].dimmed === true;
      const isLit = scenarioEdgeOpacity(state, state, LIT) === LIT;
      expect(isLit).toBe(!isDimmed);
    }
  });
});
