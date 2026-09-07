import { describe, expect, it } from "vitest";
import type { MetricTree } from "@/types/metricTree";
import { leverConflicts } from "./leverConflicts";

/** revenue → profit → margin_score; cost → profit. */
const tree = {
  nodes: [
    { id: "orders.revenue" },
    { id: "orders.cost" },
    { id: "orders.profit" },
    { id: "orders.margin_score" }
  ],
  edges: [
    { from: "orders.revenue", to: "orders.profit" },
    { from: "orders.cost", to: "orders.profit" },
    { from: "orders.profit", to: "orders.margin_score" }
  ]
} as unknown as MetricTree;

describe("leverConflicts", () => {
  it("finds nothing for a single lever", () => {
    expect(leverConflicts(tree, ["orders.revenue"])).toEqual([]);
  });

  it("finds nothing for independent levers", () => {
    expect(leverConflicts(tree, ["orders.revenue", "orders.cost"])).toEqual([]);
  });

  it("flags a direct downstream lever", () => {
    expect(leverConflicts(tree, ["orders.revenue", "orders.profit"])).toEqual([
      { upstream: "orders.revenue", downstream: "orders.profit" }
    ]);
  });

  it("flags a transitive downstream lever", () => {
    expect(leverConflicts(tree, ["orders.revenue", "orders.margin_score"])).toEqual([
      { upstream: "orders.revenue", downstream: "orders.margin_score" }
    ]);
  });

  it("does not treat the same lever twice as a conflict", () => {
    expect(leverConflicts(tree, ["orders.revenue", "orders.revenue"])).toEqual([]);
  });

  it("ignores unknown ids", () => {
    expect(leverConflicts(tree, ["orders.revenue", "orders.nope"])).toEqual([]);
  });

  it("terminates on a cyclic tree", () => {
    const cyclic = {
      nodes: [{ id: "a" }, { id: "b" }],
      edges: [
        { from: "a", to: "b" },
        { from: "b", to: "a" }
      ]
    } as unknown as MetricTree;
    expect(leverConflicts(cyclic, ["a", "b"])).toEqual([
      { upstream: "a", downstream: "b" },
      { upstream: "b", downstream: "a" }
    ]);
  });
});
