// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import type { WmMeasureBreakdownEvent } from "@/types/worldModel";
import { applyBreakdownEvent } from "./useWorldModel";

const init: WmMeasureBreakdownEvent = {
  kind: "init",
  root: "store.revenue",
  nodes: [
    {
      id: "store.revenue",
      view: "store",
      measure: "revenue",
      label: "Revenue",
      measure_type: "number",
      is_composite: true,
      is_root: true,
      expr: null
    },
    {
      id: "store.orders",
      view: "store",
      measure: "orders",
      label: "Orders",
      measure_type: "sum",
      is_composite: false,
      is_root: false,
      expr: null
    }
  ],
  edges: [{ from: "store.orders", to: "store.revenue", operator: "mul", sign: 1 }]
};

describe("applyBreakdownEvent", () => {
  it("seeds nodes with null values on init", () => {
    const s = applyBreakdownEvent(null, init);
    expect(s.nodes).toHaveLength(2);
    expect(s.nodes.every((n) => n.value === null)).toBe(true);
    expect(s.root).toBe("store.revenue");
  });

  it("fills a node value on a value event", () => {
    const seeded = applyBreakdownEvent(null, init);
    const next = applyBreakdownEvent(seeded, {
      kind: "value",
      node_id: "store.orders",
      value: "42",
      unvalued_reason: null
    });
    expect(next.nodes.find((n) => n.id === "store.orders")?.value).toBe("42");
    // unrelated node untouched
    expect(next.nodes.find((n) => n.id === "store.revenue")?.value).toBe(null);
  });

  it("records an unvalued reason", () => {
    const seeded = applyBreakdownEvent(null, init);
    const next = applyBreakdownEvent(seeded, {
      kind: "value",
      node_id: "store.orders",
      value: null,
      unvalued_reason: "no join path to instance"
    });
    expect(next.nodes.find((n) => n.id === "store.orders")?.unvalued_reason).toBe(
      "no join path to instance"
    );
  });
});
