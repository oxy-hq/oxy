import { describe, expect, it } from "vitest";
import { formatNumber, formatPercent, formatSigned, shortMeasureName } from "./measureFormat";

describe("formatNumber", () => {
  it("keeps two decimals at currency scale", () => {
    expect(formatNumber(0)).toBe("0.00");
    expect(formatNumber(1)).toBe("1.00");
    expect(formatNumber(-589.39)).toBe("-589.39");
    expect(formatNumber(999.999)).toBe("1000.00");
  });

  it("abbreviates thousands and millions", () => {
    expect(formatNumber(1_000)).toBe("1.00k");
    expect(formatNumber(-649.97 * 1_000)).toBe("-649.97k");
    expect(formatNumber(2_500_000)).toBe("2.50M");
  });

  // The reported anomaly's rate driver: two decimals rendered the one row whose
  // movement mattered as "Δ +0.00 (0.10 → 0.10)".
  it("keeps a sub-1 rate's movement visible", () => {
    expect(formatNumber(0.0964)).toBe("0.0964");
    expect(formatNumber(0.1002)).toBe("0.100");
    expect(formatNumber(0.0038)).toBe("0.00380");
    expect(formatNumber(-0.0038)).toBe("-0.00380");
  });

  it("renders an exact zero as 0.00 rather than as a tiny value", () => {
    expect(formatNumber(0)).toBe("0.00");
    expect(formatNumber(-0)).toBe("0.00");
  });

  // Deliberate: a number this small has no honest fixed-point rendering at three
  // significant figures, and "0.00" would claim nothing moved.
  it("falls into exponential form below ~1e-6", () => {
    expect(formatNumber(1e-7)).toBe("1.00e-7");
  });
});

describe("formatSigned", () => {
  it("always shows the sign", () => {
    expect(formatSigned(2.11)).toBe("+2.11");
    expect(formatSigned(-62.68)).toBe("-62.68");
    expect(formatSigned(0)).toBe("+0.00");
  });

  it("inherits the sub-1 precision rule", () => {
    expect(formatSigned(0.0038)).toBe("+0.00380");
  });
});

describe("formatPercent", () => {
  it("renders a share to two decimals", () => {
    expect(formatPercent(0.0964)).toBe("9.64%");
    expect(formatPercent(0.1002)).toBe("10.02%");
    expect(formatPercent(0)).toBe("0.00%");
    expect(formatPercent(0.9999)).toBe("99.99%");
  });

  it("does not collapse a tiny-but-nonzero share to 0.00%", () => {
    expect(formatPercent(0.00001)).toBe("0.00100%");
  });

  // A passthrough pair is not guaranteed to be a part and its whole: items per
  // order is a plain multiple, and "230.00%" would invite reading it as a share.
  it("renders a ratio of 1 or more as a multiple, not a percentage", () => {
    expect(formatPercent(2.3)).toBe("2.30");
    expect(formatPercent(1)).toBe("1.00");
    expect(formatPercent(-2.3)).toBe("-2.30");
  });
});

describe("shortMeasureName", () => {
  it("drops the view prefix", () => {
    expect(shortMeasureName("sales.total_gross_sales")).toBe("total_gross_sales");
  });

  it("passes an unqualified name through", () => {
    expect(shortMeasureName("total_gross_sales")).toBe("total_gross_sales");
  });
});
