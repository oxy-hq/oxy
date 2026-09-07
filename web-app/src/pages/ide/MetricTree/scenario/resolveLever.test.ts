import { describe, expect, it } from "vitest";
import { resolveLever } from "./resolveLever";

const baseline = { "inventory.days_in_stock": 14, "orders.free": 0 };

describe("resolveLever", () => {
  it("reads a bare number as an absolute target", () => {
    expect(resolveLever({ nodeId: "inventory.days_in_stock", raw: "11" }, baseline)).toEqual({
      nodeId: "inventory.days_in_stock",
      delta: -3
    });
  });

  it("reads a percentage against the baseline", () => {
    expect(resolveLever({ nodeId: "inventory.days_in_stock", raw: "+50%" }, baseline)).toEqual({
      nodeId: "inventory.days_in_stock",
      delta: 7
    });
  });

  it("reads a negative percentage", () => {
    expect(resolveLever({ nodeId: "inventory.days_in_stock", raw: "-50%" }, baseline)).toEqual({
      nodeId: "inventory.days_in_stock",
      delta: -7
    });
  });

  it("reads an explicitly signed number as a raw delta", () => {
    expect(resolveLever({ nodeId: "inventory.days_in_stock", raw: "+3" }, baseline)).toEqual({
      nodeId: "inventory.days_in_stock",
      delta: 3
    });
  });

  it("refuses a percentage when the baseline is zero", () => {
    expect(resolveLever({ nodeId: "orders.free", raw: "+10%" }, baseline)).toEqual({
      nodeId: "orders.free",
      error: "zero_baseline"
    });
  });

  it("refuses an absolute target with no baseline to subtract from", () => {
    expect(resolveLever({ nodeId: "orders.unknown", raw: "11" }, baseline)).toEqual({
      nodeId: "orders.unknown",
      error: "no_baseline"
    });
  });

  it("accepts a raw delta even with no baseline", () => {
    expect(resolveLever({ nodeId: "orders.unknown", raw: "+3" }, baseline)).toEqual({
      nodeId: "orders.unknown",
      delta: 3
    });
  });

  it("refuses nonsense", () => {
    expect(resolveLever({ nodeId: "inventory.days_in_stock", raw: "abc" }, baseline)).toEqual({
      nodeId: "inventory.days_in_stock",
      error: "not_a_number"
    });
  });

  it("treats a no-op as no change rather than a zero delta", () => {
    expect(resolveLever({ nodeId: "inventory.days_in_stock", raw: "14" }, baseline)).toEqual({
      nodeId: "inventory.days_in_stock",
      error: "no_change"
    });
  });
});
