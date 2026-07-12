import { describe, expect, it } from "vitest";
import { formatDuration, formatTimeAgo } from "./format";

describe("formatTimeAgo", () => {
  // Regression guard: an unparseable/empty ClickHouse timestamp used to reach
  // date-fns `formatDistanceToNow`, throw `RangeError: Invalid time value`, and
  // render-crash the whole trace page. It must degrade to an em dash instead.
  it("returns an em dash for missing or invalid input instead of throwing", () => {
    expect(formatTimeAgo("")).toBe("—");
    expect(formatTimeAgo(undefined)).toBe("—");
    expect(formatTimeAgo(null)).toBe("—");
    expect(formatTimeAgo("not-a-date")).toBe("—");
    expect(formatTimeAgo("0000-00-00 00:00:00.000000")).toBe("—");
  });

  it("formats a valid ISO-8601 timestamp without throwing", () => {
    expect(formatTimeAgo(new Date().toISOString())).toContain("ago");
  });
});

describe("formatDuration", () => {
  it("returns an em dash for non-finite or negative input", () => {
    expect(formatDuration(Number.NaN)).toBe("—");
    expect(formatDuration(Number.POSITIVE_INFINITY)).toBe("—");
    expect(formatDuration(-5)).toBe("—");
  });

  it("formats sub-second, second, and minute ranges", () => {
    expect(formatDuration(0.5)).toBe("500µs");
    expect(formatDuration(250)).toBe("250ms");
    expect(formatDuration(1500)).toBe("1.50s");
    expect(formatDuration(90000)).toBe("1.5m");
  });
});
