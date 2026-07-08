// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import type { WmInstanceDetailEvent } from "@/types/worldModel";
import { applyInstanceDetailEvent, type WmInstanceDetailState } from "./useWorldModel";

const EMPTY: WmInstanceDetailState = {
  data: null,
  pendingMeasureNames: null,
  pendingMeasures: [],
  initialized: false
};

const measureNames: WmInstanceDetailEvent = {
  kind: "measure_names",
  measure_names: [
    { name: "total_order_value", measure_type: "custom", label: "Order Value" },
    { name: "total_items", measure_type: "count", label: "Line Items" }
  ]
};

const init: WmInstanceDetailEvent = {
  kind: "init",
  entity_id: "order",
  key_value: "1",
  display: "1",
  attributes: [{ name: "order_id", value: "1", label: "Order ID" }]
};

function fold(
  state: WmInstanceDetailState,
  events: WmInstanceDetailEvent[]
): WmInstanceDetailState {
  return events.reduce(applyInstanceDetailEvent, state);
}

describe("applyInstanceDetailEvent", () => {
  it("seeds skeleton rows (null values) from measure_names on init", () => {
    const s = fold(EMPTY, [measureNames, init]);
    expect(s.data?.computed_measures).toHaveLength(2);
    expect(s.data?.computed_measures.every((m) => m.value === null)).toBe(true);
    expect(s.data?.entity_id).toBe("order");
  });

  it("fills a measure value that arrives after init", () => {
    const s = fold(EMPTY, [
      measureNames,
      init,
      {
        kind: "measure",
        computed_measures: [
          {
            name: "total_items",
            measure_type: "count",
            value: "3",
            fiber_count: 3,
            label: "Line Items"
          }
        ]
      }
    ]);
    const items = s.data?.computed_measures.find((m) => m.name === "total_items");
    expect(items?.value).toBe("3");
    // untouched sibling stays a skeleton
    expect(s.data?.computed_measures.find((m) => m.name === "total_order_value")?.value).toBe(null);
  });

  // Regression: a measure group whose SQL failed to compile is emitted instantly
  // (no DB round-trip) and can arrive BEFORE init. It must be buffered and merged
  // into the skeletons on init — otherwise the row pulses as a skeleton forever.
  it("does not drop a measure event that races ahead of init", () => {
    const s = fold(EMPTY, [
      measureNames,
      {
        kind: "measure",
        computed_measures: [
          {
            name: "total_order_value",
            measure_type: "custom",
            value: "—",
            fiber_count: 1,
            label: "Order Value"
          }
        ]
      },
      init
    ]);
    const orderValue = s.data?.computed_measures.find((m) => m.name === "total_order_value");
    expect(orderValue?.value).toBe("—");
    // order is still preserved and the other row remains a skeleton
    expect(s.data?.computed_measures.map((m) => m.name)).toEqual([
      "total_order_value",
      "total_items"
    ]);
    expect(s.data?.computed_measures.find((m) => m.name === "total_items")?.value).toBe(null);
    // the buffer is drained once applied
    expect(s.pendingMeasures).toHaveLength(0);
  });

  it("appends child fibers and records parent promotions after init", () => {
    const s = fold(EMPTY, [
      measureNames,
      init,
      {
        kind: "child",
        child: {
          promotion: "order_item → order",
          fiber_count: 3,
          sample: ["1 · 1"],
          sample_keys: ['["1","1"]']
        }
      },
      {
        kind: "parent",
        promotes_to: [{ promotion: "order → customer", key: "16853", display: "Jonathan Smith" }]
      }
    ]);
    expect(s.data?.receives_from).toHaveLength(1);
    expect(s.data?.promotes_to[0]?.key).toBe("16853");
  });
});
