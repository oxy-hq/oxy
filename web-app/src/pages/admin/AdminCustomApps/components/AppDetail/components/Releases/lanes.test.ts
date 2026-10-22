import { describe, expect, it } from "vitest";
import {
  clsText,
  deltaText,
  laneOf,
  position,
  rateText,
  reasonText,
  releaseLabel,
  verdictTone
} from "./lanes";

const SINCE = "2026-10-14T00:00:00Z";
const UNTIL = "2026-10-21T00:00:00Z";

describe("position", () => {
  it("places an instant on the window's axis and clamps outside it", () => {
    expect(position("2026-10-17T12:00:00Z", SINCE, UNTIL)).toBe(50);
    expect(position("2026-10-01T00:00:00Z", SINCE, UNTIL)).toBe(0);
    expect(position("2026-11-01T00:00:00Z", SINCE, UNTIL)).toBe(100);
    expect(position("not a date", SINCE, UNTIL)).toBe(0);
  });
});

describe("laneOf", () => {
  it("puts every sandbox on one lane", () => {
    expect(laneOf("production")).toBe("production");
    expect(laneOf("staging")).toBe("staging");
    expect(laneOf("dev-ann")).toBe("sandboxes");
  });
});

describe("verdictTone", () => {
  it("colours only what is wrong", () => {
    expect(verdictTone("worse")).toBe("danger");
    expect(verdictTone("slower")).toBe("warn");
    // Not worse is not healthy: no green.
    expect(verdictTone("not_worse")).toBe("muted");
    expect(verdictTone("not_comparable")).toBe("muted");
  });
});

describe("number text", () => {
  it("prints basis points and CLS from integers", () => {
    expect(rateText(1696)).toBe("16.96%");
    expect(rateText(5)).toBe("0.05%");
    expect(rateText(0)).toBe("0.00%");
    expect(rateText(null)).toBe("—");
    expect(clsText(70)).toBe("0.070");
    expect(clsText(1250)).toBe("1.250");
  });

  it("signs a delta in the metric's unit", () => {
    expect(deltaText(0, 1696, "bp")).toBe("+16.96 pt");
    expect(deltaText(2300, 450, "ms")).toBe("−1,850 ms");
    expect(deltaText(10, 10, "cls_milli")).toBe("±0.000");
    expect(deltaText(null, 10, "ms")).toBeNull();
  });

  it("states a reason with both numbers", () => {
    expect(
      reasonText({
        metric: "lcp",
        screen: "/store",
        before: 1018,
        after: 2618,
        unit: "ms",
        rule: "r"
      })
    ).toBe("LCP p75 on /store 1,018 ms → 2,618 ms");
  });
});

describe("releaseLabel", () => {
  it("shortens a sha and keeps any other publish id whole", () => {
    expect(releaseLabel("2e7ae899f6ee1234abcd")).toBe("2e7ae89");
    expect(releaseLabel("pulse-r4-2e7ae899f6ee")).toBe("pulse-r4-2e7ae899f6ee");
    expect(releaseLabel("v12")).toBe("v12");
  });
});
