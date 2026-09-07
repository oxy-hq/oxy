import { describe, expect, it } from "vitest";
import { leverOutsideAnchor, pickTimeDimension, usableTimeDimensions } from "./pickTimeDimension";

const byView = {
  astronauts: ["astronauts.birth_date"],
  checks: ["checks.opened_at", "checks.closed_at"],
  menu_items: []
};

describe("pickTimeDimension", () => {
  it("picks a dimension from the lever's own view", () => {
    expect(pickTimeDimension([{ nodeId: "checks.alcoholic_revenue", raw: "+3" }], byView)).toBe(
      "checks.opened_at"
    );
  });

  // The bug this exists to prevent: the first dimension in the layer belonged
  // to an unrelated view, so the baseline query failed and every impacted
  // measure was reclassified as unvalued.
  it("refuses to borrow a dimension from an unrelated view", () => {
    expect(pickTimeDimension([{ nodeId: "menu_items.avg_unit_price", raw: "-3" }], byView)).toBe(
      null
    );
  });

  it("returns null when no lever is pinned", () => {
    expect(pickTimeDimension([], byView)).toBe(null);
  });

  it("returns null when the lever's view is absent from the map", () => {
    expect(pickTimeDimension([{ nodeId: "unknown.measure", raw: "+1" }], byView)).toBe(null);
  });

  it("finds a usable view when an earlier lever's view has none", () => {
    const levers = [
      { nodeId: "menu_items.avg_unit_price", raw: "-3" },
      { nodeId: "checks.alcoholic_revenue", raw: "+3" }
    ];
    expect(pickTimeDimension(levers, byView)).toBe("checks.opened_at");
  });
});

describe("usableTimeDimensions", () => {
  const byView = {
    checks: ["checks.check_date"],
    store_days: ["store_days.business_date"],
    astronauts: ["astronauts.birth_date"]
  };

  // The bug this exists to stop: a `checks` lever grouped by
  // `store_days.business_date` joins the coarser view in on `location_id`
  // alone, so every measure comes back as the window's total repeated on every
  // date. The fit then refuses on 26,280 observations that are one value 73
  // times over.
  it("offers only dimensions on a pinned lever's own view", () => {
    expect(usableTimeDimensions([{ nodeId: "checks.total_guests", raw: "+5%" }], byView)).toEqual([
      "checks.check_date"
    ]);
  });

  it("spans every lever's view when levers straddle two", () => {
    const dims = usableTimeDimensions(
      [
        { nodeId: "checks.total_guests", raw: "+5%" },
        { nodeId: "store_days.marketing_spend", raw: "+5%" }
      ],
      byView
    );
    expect(new Set(dims)).toEqual(new Set(["checks.check_date", "store_days.business_date"]));
  });

  it("offers the whole layer when nothing is pinned yet", () => {
    // Nothing to be foreign to, and no query runs until a lever exists.
    expect(usableTimeDimensions([], byView)).toHaveLength(3);
  });

  it("offers nothing when the lever's view declares no time dimension", () => {
    expect(usableTimeDimensions([{ nodeId: "menu_items.price", raw: "+5%" }], byView)).toEqual([]);
  });
});

describe("leverOutsideAnchor", () => {
  it("is true for a lever on a view the window is not anchored on", () => {
    // The gap `usableTimeDimensions` leaves open: it spans EVERY lever's view,
    // so a dimension from one of them passes the foreign check while the
    // levers on the other view get no baseline at all.
    expect(leverOutsideAnchor("checks.total_guests", "store_days.business_date")).toBe(true);
  });

  it("is false for a lever on the anchored view", () => {
    expect(leverOutsideAnchor("store_days.marketing_spend", "store_days.business_date")).toBe(
      false
    );
  });

  it("is false with nothing anchored, because delta-only is not a mismatch", () => {
    // No window means no baseline for ANY lever, which the delta-only banner
    // already says once. Repeating it per lever would be noise.
    expect(leverOutsideAnchor("checks.total_guests", null)).toBe(false);
  });
});
