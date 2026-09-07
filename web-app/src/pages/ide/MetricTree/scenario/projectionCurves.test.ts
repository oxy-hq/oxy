import { describe, expect, it } from "vitest";
import type { MeasureProjection } from "@/types/metricTree";
import { addDays, scenarioCurve } from "./projectionCurves";

/** `n` daily forecast buckets of `value`, starting 2026-08-01. */
function projection(n: number, value = 100, step = 1): MeasureProjection {
  return {
    measure: "store_days.net_sales",
    history: [],
    forecast: Array.from({ length: n }, (_, i) => ({
      date: addDays("2026-08-01", i * step),
      point: value,
      lower: value - 10,
      upper: value + 10
    })),
    seasonality: [7]
  };
}

const base = {
  projection: projection(10),
  baselineValue: 1000,
  delta: 100,
  confidence: "estimated" as const
};

describe("scenarioCurve", () => {
  it("shifts every bucket by the lever's proportion", () => {
    const curve = scenarioCurve(base);
    expect(curve.kind).toBe("curve");
    if (curve.kind !== "curve") return;
    // +100 on a window total of 1000 is +10%, applied to each bucket.
    expect(curve.points.every((p) => Math.abs(p.value - 110) < 1e-9)).toBe(true);
    expect(curve.landsAt).toBe("2026-08-01");
  });

  it("moves the horizon total by the same proportion, not by the delta", () => {
    const curve = scenarioCurve(base);
    if (curve.kind !== "curve") throw new Error("expected a curve");
    const baselineTotal = 10 * 100;
    const scenarioTotal = curve.points.reduce((sum, p) => sum + p.value, 0);
    // The lever is a sustained proportional change, so a horizon of a
    // different length than the window scales — it does not receive a copy of
    // the window's absolute delta.
    expect(scenarioTotal / baselineTotal).toBeCloseTo(1.1, 10);
  });

  it("holds the baseline until the lag lands, then separates", () => {
    const curve = scenarioCurve({ ...base, lagDays: 3 });
    if (curve.kind !== "curve") throw new Error("expected a curve");
    expect(curve.landsAt).toBe("2026-08-04");
    expect(curve.points.slice(0, 3).map((p) => p.value)).toEqual([100, 100, 100]);
    expect(curve.points[3]?.value).toBeCloseTo(110, 9);
  });

  it("lands a mid-bucket lag in the bucket already running, not the next one", () => {
    // Weekly buckets; a 10-day lag falls inside the second week.
    const curve = scenarioCurve({ ...base, projection: projection(6, 100, 7), lagDays: 10 });
    if (curve.kind !== "curve") throw new Error("expected a curve");
    expect(curve.landsAt).toBe("2026-08-08");
  });

  it("carries a negative delta through as a downward shift", () => {
    const curve = scenarioCurve({ ...base, delta: -250 });
    if (curve.kind !== "curve") throw new Error("expected a curve");
    expect(curve.points[0]?.value).toBe(75);
  });

  /** The single most important assertion here: an unsizable impact must never
   *  render as the two curves lying on top of each other. */
  it("refuses an unquantifiable impact rather than shifting by its zero", () => {
    const curve = scenarioCurve({ ...base, delta: 0, confidence: "unquantifiable" });
    expect(curve).toEqual({ kind: "refused", reason: "unquantifiable" });
  });

  it("refuses when nothing propagated to the measure", () => {
    expect(scenarioCurve({ ...base, delta: undefined, confidence: undefined })).toEqual({
      kind: "refused",
      reason: "unmoved"
    });
  });

  it("refuses when the series produced no forecast to shift", () => {
    const refused: MeasureProjection = {
      measure: "store_days.net_sales",
      history: [{ date: "2026-07-01", value: 1 }],
      forecast: [],
      refusal: "only 10 measured bucket(s) of history, need 56",
      // A refused series was never decomposed, so it carries no periods.
      seasonality: []
    };
    expect(scenarioCurve({ ...base, projection: refused })).toEqual({
      kind: "refused",
      reason: "no_forecast"
    });
  });

  it("refuses without a baseline value to take a proportion of", () => {
    expect(scenarioCurve({ ...base, baselineValue: undefined })).toEqual({
      kind: "refused",
      reason: "no_baseline_value"
    });
  });

  it("refuses a zero baseline instead of dividing by it", () => {
    expect(scenarioCurve({ ...base, baselineValue: 0 })).toEqual({
      kind: "refused",
      reason: "zero_baseline"
    });
  });

  /** Two identical curves would say "this lever changes nothing"; it changes
   *  nothing YET, which is a different claim. */
  it("refuses when the effect lands past the last bucket", () => {
    expect(scenarioCurve({ ...base, lagDays: 30 })).toEqual({
      kind: "refused",
      reason: "lands_after_horizon"
    });
  });

  it("treats a lag landing inside the final bucket as inside the horizon", () => {
    // Weekly buckets, six of them: the last starts 2026-09-05 and runs to the
    // 11th, so a 40-day lag (2026-09-10) is still inside it.
    const curve = scenarioCurve({ ...base, projection: projection(6, 100, 7), lagDays: 40 });
    if (curve.kind !== "curve") throw new Error("expected a curve");
    expect(curve.landsAt).toBe("2026-09-05");
    expect(curve.points.at(-1)?.value).toBeCloseTo(110, 9);
    expect(curve.points.at(-2)?.value).toBe(100);
  });

  it("carries the impact's confidence through for the renderer", () => {
    const curve = scenarioCurve({ ...base, confidence: "exact" });
    if (curve.kind !== "curve") throw new Error("expected a curve");
    expect(curve.confidence).toBe("exact");
  });
});

describe("addDays", () => {
  it("crosses a month boundary", () => {
    expect(addDays("2026-01-31", 1)).toBe("2026-02-01");
  });

  it("crosses a leap day", () => {
    expect(addDays("2024-02-28", 1)).toBe("2024-02-29");
  });
});
