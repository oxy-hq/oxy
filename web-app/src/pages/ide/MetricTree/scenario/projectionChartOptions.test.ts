import type { LineSeriesOption } from "echarts";
import { describe, expect, it } from "vitest";
import type { MeasureProjection } from "@/types/metricTree";
import { projectionChartOptions } from "./projectionChartOptions";
import type { ScenarioCurve } from "./projectionCurves";

const colors = {
  actual: "#000000",
  baseline: "#111111",
  scenario: "#222222",
  band: "#333333",
  axis: "#444444",
  grid: "#555555"
};

/** `n` daily buckets ending the day before `forecast` starts. */
function longHistory(n: number) {
  return Array.from({ length: n }, (_, i) => ({
    date: `h${i}`,
    value: 100 + i
  }));
}

function projection(overrides: Partial<MeasureProjection> = {}): MeasureProjection {
  return {
    measure: "v.a",
    history: [
      { date: "2026-07-30", value: 90 },
      { date: "2026-07-31", value: 100 }
    ],
    forecast: [
      { date: "2026-08-01", point: 100, lower: 90, upper: 110 },
      { date: "2026-08-02", point: 102, lower: 92, upper: 112 }
    ],
    seasonality: [7],
    ...overrides
  };
}

const curve: ScenarioCurve = {
  kind: "curve",
  landsAt: "2026-08-01",
  confidence: "estimated",
  points: [
    { date: "2026-08-01", value: 110 },
    { date: "2026-08-02", value: 112 }
  ]
};

function build(p: MeasureProjection, c: ScenarioCurve) {
  const options = projectionChartOptions({
    projection: p,
    curve: c,
    colors,
    format: String
  });
  return options.series as LineSeriesOption[];
}

const names = (series: LineSeriesOption[]) => series.map((s) => s.name);

describe("projectionChartOptions", () => {
  it("draws actual, baseline forecast, band and scenario", () => {
    expect(names(build(projection(), curve))).toEqual([
      "band-floor",
      "band-height",
      "actual",
      "baseline forecast",
      "scenario"
    ]);
  });

  /** The chart's version of "unquantifiable ≠ 0": a refused curve contributes
   *  no series, rather than a line lying on top of the baseline. */
  it("omits the scenario series entirely when the curve was refused", () => {
    const series = build(projection(), { kind: "refused", reason: "unquantifiable" });
    expect(names(series)).not.toContain("scenario");
  });

  it("omits the band unless every bucket carries one", () => {
    const partial = projection({
      forecast: [
        { date: "2026-08-01", point: 100, lower: 90, upper: 110 },
        { date: "2026-08-02", point: 102, lower: null, upper: null }
      ]
    });
    const series = build(partial, curve);
    expect(names(series)).not.toContain("band-floor");
    expect(names(series)).toContain("baseline forecast");
  });

  it("joins the forecast to the last historical point so the line has no gap", () => {
    const series = build(projection(), curve);
    const forecast = series.find((s) => s.name === "baseline forecast");
    // Two history buckets: a null, then the seam value, then the forecast.
    expect(forecast?.data).toEqual([null, 100, 100, 102]);
  });

  it("keeps the band's helper series out of the legend", () => {
    const options = projectionChartOptions({
      projection: projection(),
      curve,
      colors,
      format: String
    });
    const legend = options.legend as { data: string[] };
    expect(legend.data).toEqual(["actual", "baseline forecast", "scenario"]);
  });

  /**
   * The scenario curve is the baseline forecast multiplied by a constant, so
   * it is exactly as much a forecast as the baseline is. `exact` describes the
   * propagation EDGE — how the lever's change reaches this measure — and
   * drawing that as a solid line made the scenario half of the chart claim a
   * certainty about the future the baseline beside it never claims. The
   * exact/estimated distinction stays where it belongs: `ConfidenceMark`, on
   * the impact.
   */
  it("dashes the scenario forecast whatever the propagation's confidence", () => {
    for (const confidence of ["exact", "estimated", "unquantifiable"] as const) {
      const scenario = build(projection(), { ...curve, confidence }).find(
        (s) => s.name === "scenario"
      );
      expect(scenario?.lineStyle?.type).toBe("dashed");
    }
  });

  it("draws the scenario forecast in the same stroke as the baseline forecast", () => {
    const series = build(projection(), { ...curve, confidence: "exact" });
    const scenario = series.find((s) => s.name === "scenario");
    const baseline = series.find((s) => s.name === "baseline forecast");
    expect(scenario?.lineStyle?.type).toBe(baseline?.lineStyle?.type);
    // Same dash, different colour — the accent is what separates them, not a
    // claim about how sure either half is.
    expect(scenario?.lineStyle?.color).toBe(colors.scenario);
    expect(scenario?.lineStyle?.color).not.toBe(baseline?.lineStyle?.color);
  });

  it("separates the actual line from the baseline forecast by colour, not just dash", () => {
    const series = build(projection(), curve);
    const actual = series.find((s) => s.name === "actual");
    const baseline = series.find((s) => s.name === "baseline forecast");
    expect(actual?.lineStyle?.color).toBe(colors.actual);
    expect(baseline?.lineStyle?.color).toBe(colors.baseline);
    expect(actual?.lineStyle?.color).not.toBe(baseline?.lineStyle?.color);
  });

  describe("zoom window", () => {
    function options(historyLength: number) {
      return projectionChartOptions({
        projection: projection({ history: longHistory(historyLength) }),
        curve,
        colors,
        format: String
      });
    }

    /** The fit needs a year of dailies; the *view* does not, and rendering one
     *  into a side panel is what made the chart unreadable. */
    it("opens on the seam when the history dwarfs the forecast", () => {
      const zoom = options(365).dataZoom as { startValue: number; endValue: number }[];
      // 2 forecast buckets, so the 24-bucket floor sets the context width.
      expect(zoom.map((z) => [z.startValue, z.endValue])).toEqual([
        [341, 366],
        [341, 366]
      ]);
    });

    it("scales the visible history with the horizon", () => {
      const long = projectionChartOptions({
        projection: {
          measure: "v.a",
          history: longHistory(365),
          forecast: Array.from({ length: 60 }, (_, i) => ({
            date: `f${i}`,
            point: 100,
            lower: 90,
            upper: 110
          })),
          seasonality: [7]
        },
        curve,
        colors,
        format: String
      });
      const zoom = long.dataZoom as { startValue: number }[];
      expect(zoom[0]?.startValue).toBe(365 - 120);
    });

    /** No slider when nothing is hidden — the control would only advertise
     *  history that isn't there. */
    it("omits the zoom entirely when the whole series already fits", () => {
      expect(options(20).dataZoom).toBeUndefined();
      const grid = options(20).grid as { bottom: number };
      expect(grid.bottom).toBe(24);
    });

    it("gives the plot back the room the slider takes", () => {
      const grid = options(365).grid as { bottom: number };
      expect(grid.bottom).toBe(52);
    });
  });
});
