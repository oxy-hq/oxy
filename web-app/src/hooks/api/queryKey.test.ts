import { describe, expect, it } from "vitest";
import queryKeys from "./queryKey";

const BASE = [
  "proj-1",
  "main",
  ["store_days.marketing_spend"],
  "store_days.business_date",
  ["2025-07-20", "2026-07-19"] as [string, string],
  null
] as const;

describe("metricTree.projection key", () => {
  it("separates granularity and horizon", () => {
    const day = queryKeys.metricTree.projection(...BASE, "day", 30);
    const week = queryKeys.metricTree.projection(...BASE, "week", 30);
    const longer = queryKeys.metricTree.projection(...BASE, "day", 60);
    expect(day).not.toEqual(week);
    expect(day).not.toEqual(longer);
  });
});
