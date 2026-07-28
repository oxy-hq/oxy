import { describe, expect, it } from "vitest";
import type { WorldModel, WorldModelEntity } from "@/types/worldModel";
import {
  dimensionNodeSelection,
  formatSegment,
  measureNodeSelection,
  segmentQuestion
} from "./worldModelNav";

function entity(
  over: Partial<WorldModelEntity> & Pick<WorldModelEntity, "id" | "view">
): WorldModelEntity {
  return {
    label: over.id,
    depth: 0,
    dimensions: [],
    own_measures: [],
    induced_measures: [],
    ...over
  };
}

const MODEL: WorldModel = {
  edges: [],
  entities: [
    entity({
      id: "order",
      view: "order",
      dimensions: [{ name: "store_region", dim_type: "string" }],
      own_measures: [{ name: "revenue", measure_type: "sum", additivity: "additive" }]
    }),
    entity({
      id: "category",
      view: "fruit_category",
      induced_measures: [
        {
          name: "total_revenue",
          measure_type: "sum",
          additivity: "additive",
          promoted_from: "order",
          path: ["order", "fruit", "fruit_category"]
        }
      ]
    })
  ]
};

describe("measureNodeSelection", () => {
  it("resolves a measure to the entity that declares it", () => {
    expect(measureNodeSelection(MODEL, "order.revenue")).toEqual({
      kind: "measure",
      entityId: "order",
      measureName: "revenue",
      induced: false
    });
  });

  it("resolves an induced measure to the entity it is promoted onto, from that view", () => {
    expect(measureNodeSelection(MODEL, "order.total_revenue")).toEqual({
      kind: "measure",
      entityId: "category",
      measureName: "total_revenue",
      induced: true,
      promotedFrom: "order"
    });
  });

  it("returns null when the world model does not host the measure", () => {
    expect(measureNodeSelection(MODEL, "unknown.metric")).toBeNull();
    expect(measureNodeSelection(MODEL, "revenue")).toBeNull();
  });
});

describe("dimensionNodeSelection", () => {
  it("resolves a bare dimension name on the hosting entity", () => {
    expect(dimensionNodeSelection(MODEL, "order", "store_region")).toEqual({
      kind: "dimension",
      entityId: "order",
      dimensionName: "store_region"
    });
  });

  it("tolerates a view-qualified dimension id", () => {
    expect(dimensionNodeSelection(MODEL, "order", "order.store_region")).toEqual({
      kind: "dimension",
      entityId: "order",
      dimensionName: "store_region"
    });
  });

  it("returns null for a dimension the entity does not declare", () => {
    expect(dimensionNodeSelection(MODEL, "order", "channel")).toBeNull();
  });
});

describe("segmentQuestion", () => {
  it("builds a concrete, self-contained investigation prompt", () => {
    const q = segmentQuestion({
      target: "order.revenue",
      dimension: "store_region",
      segment: "South",
      currentRate: 21,
      benchmark: 30,
      upside: 3600,
      periodDays: 90
    });
    expect(q).toContain("last 90 days");
    expect(q).toContain("revenue");
    expect(q).toContain('store_region = "South"');
    expect(q).toContain("21.0");
    expect(q).toContain("30.0");
    expect(q).toContain("3.6k");
  });

  it("states the scope, so a scoped panel's numbers aren't asked about population-wide", () => {
    const q = segmentQuestion({
      target: "order.revenue",
      dimension: "order_status",
      segment: "completed",
      currentRate: 21,
      benchmark: 30,
      upside: 3600,
      periodDays: 90,
      scope: { entity: "retail_store", key: "7" }
    });
    expect(q).toContain('within retail_store = "7"');
  });

  it("says nothing about scope when the scan was population-wide", () => {
    const q = segmentQuestion({
      target: "order.revenue",
      dimension: "order_status",
      segment: "completed",
      currentRate: 21,
      benchmark: 30,
      upside: 3600,
      periodDays: 90
    });
    expect(q).not.toContain("within");
  });

  it("tidies a float-literal segment so the agent is asked about a whole number", () => {
    const q = segmentQuestion({
      target: "order.revenue",
      dimension: "store_id",
      segment: "12.0",
      currentRate: 21,
      benchmark: 30,
      upside: 3600,
      periodDays: 90
    });
    expect(q).toContain('store_id = "12"');
  });
});

describe("formatSegment", () => {
  it("strips an all-zero fraction from a whole number", () => {
    expect(formatSegment("1.0")).toBe("1");
    expect(formatSegment("12.00")).toBe("12");
    expect(formatSegment("-3.0")).toBe("-3");
  });

  it("leaves a genuine decimal alone", () => {
    expect(formatSegment("1.5")).toBe("1.5");
    expect(formatSegment("0.01")).toBe("0.01");
    expect(formatSegment("1.10")).toBe("1.10");
  });

  it("passes non-numeric segments through untouched", () => {
    expect(formatSegment("completed")).toBe("completed");
    expect(formatSegment("NULL")).toBe("NULL");
    // Merely containing digits is not enough — this is a name, not a number.
    expect(formatSegment("v1.0")).toBe("v1.0");
    expect(formatSegment("Store 1.0 North")).toBe("Store 1.0 North");
  });

  it("does not reformat wide ids or large values", () => {
    // A Number() round-trip would lose precision here or go exponential.
    expect(formatSegment("900719925474099123.0")).toBe("900719925474099123");
    expect(formatSegment("1e21")).toBe("1e21");
  });
});
