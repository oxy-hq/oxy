import { describe, expect, it } from "vitest";
import type { SensitivityDriver } from "@/types/metricTree";
import {
  byLeverage,
  declaringView,
  formatBeta,
  formatCompact,
  formatCount,
  formatDelta,
  formatSignedPct,
  rowUnit
} from "./measureTarget";

function driver(overrides: Partial<SensitivityDriver>): SensitivityDriver {
  return {
    measure: "order.conversion",
    path: ["order.conversion", "order.revenue"],
    edge_kind: "driver",
    effective_coefficient: 0.5,
    direction: "positive",
    strength: "moderate",
    ...overrides
  };
}

describe("declaringView", () => {
  it("uses the host entity's view for a measure declared on that grain", () => {
    expect(declaringView(false, undefined, "store")).toBe("store");
  });

  it("uses the promotion source for an induced measure, not the grain it surfaced on", () => {
    // `revenue` shows on Store but is declared on Order — the metric tree knows
    // it as `order.revenue`, and Order is where its time dimensions live.
    expect(declaringView(true, "order", "store")).toBe("order");
  });

  it("is undefined when an induced measure has no recorded source", () => {
    expect(declaringView(true, undefined, "store")).toBeUndefined();
  });
});

describe("byLeverage", () => {
  it("ranks by absolute elasticity, so strong negative drivers are not buried", () => {
    const drivers = [
      driver({ measure: "a", effective_coefficient: 0.2 }),
      driver({ measure: "b", effective_coefficient: -0.9 }),
      driver({ measure: "c", effective_coefficient: 0.5 })
    ];

    expect([...drivers].sort(byLeverage).map((d) => d.measure)).toEqual(["b", "c", "a"]);
  });

  it("sinks unquantified drivers to the bottom rather than treating them as zero", () => {
    const drivers = [
      driver({ measure: "qualitative", effective_coefficient: null }),
      driver({ measure: "weak", effective_coefficient: 0.01 })
    ];

    expect([...drivers].sort(byLeverage).map((d) => d.measure)).toEqual(["weak", "qualitative"]);
  });
});

describe("formatBeta", () => {
  it("renders an em-dash for a driver the tree cannot size", () => {
    expect(formatBeta(null)).toBe("—");
    expect(formatBeta(undefined)).toBe("—");
  });

  it("renders a coefficient to two places", () => {
    expect(formatBeta(0.8163)).toBe("0.82");
    expect(formatBeta(-1.4)).toBe("-1.40");
  });
});

describe("formatCompact / formatDelta", () => {
  it("abbreviates magnitudes", () => {
    expect(formatCompact(1_250_000)).toBe("1.25M");
    expect(formatCompact(412_000)).toBe("412.0k");
    expect(formatCompact(87.42)).toBe("87.4");
    expect(formatCompact(0)).toBe("0");
  });

  it("signs a delta so it reads as a direction", () => {
    expect(formatDelta(180_000)).toBe("+180.0k");
    expect(formatDelta(-1_200)).toBe("-1.2k");
  });
});

describe("formatSignedPct", () => {
  it("expresses a value as a signed share of the base", () => {
    expect(formatSignedPct(12_000, 100_000)).toBe("+12%");
    expect(formatSignedPct(-4_000, 100_000)).toBe("-4%");
  });

  it("keeps one decimal below 10% so small shares don't collapse to 0%", () => {
    expect(formatSignedPct(3_600, 100_000)).toBe("+3.6%");
    expect(formatSignedPct(400, 100_000)).toBe("+0.4%");
  });

  it("drops trailing .0 at or above 10%", () => {
    expect(formatSignedPct(20_000, 100_000)).toBe("+20%");
  });

  it("returns null when the base can't anchor a percentage", () => {
    // A zero or negative overall value would divide-by-zero or invert the sign.
    expect(formatSignedPct(5, 0)).toBeNull();
    expect(formatSignedPct(5, -100)).toBeNull();
    expect(formatSignedPct(5, Number.NaN)).toBeNull();
  });
});

describe("formatCount", () => {
  it("never shows a trailing .0 on a whole count", () => {
    // The row-count case that made formatCompact read as "400.0 rows".
    expect(formatCount(400)).toBe("400");
    expect(formatCount(1)).toBe("1");
    expect(formatCount(0)).toBe("0");
  });

  it("rounds fractional counts to a whole number", () => {
    expect(formatCount(87.6)).toBe("88");
  });

  it("abbreviates large counts", () => {
    expect(formatCount(1_200)).toBe("1.2k");
    expect(formatCount(2_500_000)).toBe("2.5M");
  });
});

describe("rowUnit", () => {
  it("reads the same whether the view name is plural or singular", () => {
    // View names are not reliably plural, but the prose must be: a rate is
    // "per order" and a volume is "189 orders" either way.
    expect(rowUnit("orders")).toEqual({ one: "order", many: "orders" });
    expect(rowUnit("order")).toEqual({ one: "order", many: "orders" });
  });

  it("turns a snake_case view into readable prose", () => {
    expect(rowUnit("order_items")).toEqual({ one: "order item", many: "order items" });
    expect(rowUnit("Shipments")).toEqual({ one: "shipment", many: "shipments" });
  });

  it("does not strip the 's' off a word that ends in a sibilant", () => {
    // A sibilant-ending word like "business" must not be treated as a plural.
    expect(rowUnit("business")).toEqual({ one: "business", many: "businesses" });
  });
});
