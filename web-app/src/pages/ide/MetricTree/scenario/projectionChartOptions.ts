import type { EChartsOption, LineSeriesOption } from "echarts";
import type { MeasureProjection } from "@/types/metricTree";
import type { ScenarioCurve } from "./projectionCurves";

/**
 * ECharts options for the two-curve projection chart.
 *
 * Pure so the decisions that matter can be asserted without a canvas: that the
 * scenario series is *absent* when the curve was refused (rather than drawn
 * flat on the baseline), and that the prediction band is *absent* unless every
 * bucket has one (rather than closing over the gaps and implying a certainty
 * the model never expressed).
 */

export interface ProjectionChartColors {
  /** The measured history — context, so the quietest line on the chart. */
  actual: string;
  /** The baseline forecast. Brighter than `actual`: it is the thing the
   *  scenario is read against, so it has to survive next to the accent. */
  baseline: string;
  scenario: string;
  band: string;
  axis: string;
  grid: string;
}

export interface ProjectionChartArgs {
  projection: MeasureProjection;
  curve: ScenarioCurve;
  colors: ProjectionChartColors;
  /** Formatter for the y axis and the tooltip. */
  format: (value: number) => string;
}

/** A series' data padded with `null` outside its own region — ECharts breaks
 *  the line at a null, which is what keeps history and forecast visually
 *  distinct on one continuous axis. */
type Padded = (number | null)[];

export function projectionChartOptions({
  projection,
  curve,
  colors,
  format
}: ProjectionChartArgs): EChartsOption {
  const { history, forecast } = projection;
  const dates = [...history.map((p) => p.date), ...forecast.map((p) => p.date)];
  const historyLength = history.length;

  // The forecast series repeats the last historical point so the two lines
  // meet. Without it the chart shows a one-bucket gap at the seam and reads as
  // missing data.
  const seam = (values: (number | null)[]): Padded => [
    ...Array<number | null>(Math.max(0, historyLength - 1)).fill(null),
    ...(historyLength > 0 ? [history[historyLength - 1]?.value ?? null] : []),
    ...values
  ];

  const series: LineSeriesOption[] = [];

  const band = bandSeries(projection, historyLength, colors.band);
  if (band) series.push(...band);

  series.push(
    {
      name: "actual",
      type: "line",
      showSymbol: false,
      data: [...history.map((p) => p.value), ...Array<null>(forecast.length).fill(null)],
      // Thinner and dimmer than either forecast. A daily series carries a lot
      // of bucket-to-bucket noise, and at full weight that noise is the loudest
      // thing on a chart whose subject is the other half of the axis.
      lineStyle: { color: colors.actual, width: 1.5 },
      itemStyle: { color: colors.actual }
    },
    {
      name: "baseline forecast",
      type: "line",
      showSymbol: false,
      data: seam(forecast.map((p) => p.point)),
      // Dashed for the same reason `ConfidenceMark` dashes an estimate: this
      // half of the line is a model output, not a measurement, and drawn in
      // the same stroke as the actuals it reads as one.
      lineStyle: { color: colors.baseline, width: 2, type: "dashed" },
      itemStyle: { color: colors.baseline }
    }
  );

  // Only when there IS a curve. A refused one contributes no series at all —
  // the alternative, a scenario line lying exactly on the baseline, is the
  // chart claiming "this lever changes nothing" where the model said it could
  // not size the move.
  if (curve.kind === "curve") {
    series.push({
      name: "scenario",
      type: "line",
      showSymbol: false,
      data: seam(curve.points.map((p) => p.value)),
      // Dashed like the baseline forecast, and unconditionally so. This curve
      // is that same forecast multiplied by a constant, so it is exactly as
      // much a model output as the baseline is. `curve.confidence` grades the
      // propagation EDGE — how the lever's change reaches this measure — not
      // the forecast; keying the stroke off it drew the scenario solid over a
      // dashed baseline and claimed a certainty about the future that the line
      // beside it, fitted from the same series, never claimed. The
      // exact/estimated distinction has its own surface: `ConfidenceMark`, on
      // the impact it actually describes.
      lineStyle: { color: colors.scenario, width: 2, type: "dashed" },
      itemStyle: { color: colors.scenario },
      markLine: {
        silent: true,
        symbol: "none",
        label: { formatter: "effect lands", color: colors.axis, fontSize: 10 },
        lineStyle: { color: colors.scenario, type: "dotted", opacity: 0.7 },
        data: [{ xAxis: curve.landsAt }]
      }
    });
  }

  const zoom = zoomWindow(historyLength, forecast.length);

  return {
    animation: false,
    toolbox: { show: false },
    grid: {
      top: 16,
      right: 12,
      // The slider is drawn inside the chart's own box, so the plot has to give
      // up the room rather than overlap it.
      bottom: zoom ? 52 : 24,
      left: 8,
      containLabel: true
    },
    dataZoom: zoom ? dataZoom(zoom, colors) : undefined,
    tooltip: {
      trigger: "axis",
      valueFormatter: (value) => (typeof value === "number" ? format(value) : "—")
    },
    legend: {
      bottom: 0,
      itemHeight: 8,
      itemWidth: 14,
      textStyle: { color: colors.axis, fontSize: 10 },
      // The band's two helper series are stacking machinery, not things a
      // reader picks off a legend.
      data: series
        .filter((s) => s.name !== BAND_FLOOR && s.name !== BAND_HEIGHT)
        .map((s) => String(s.name))
    },
    xAxis: {
      type: "category",
      data: dates,
      axisLabel: { color: colors.axis, fontSize: 10, hideOverlap: true },
      axisLine: { lineStyle: { color: colors.grid } },
      axisTick: { show: false }
    },
    yAxis: {
      type: "value",
      scale: true,
      axisLabel: { color: colors.axis, fontSize: 10, formatter: (v: number) => format(v) },
      splitLine: { lineStyle: { color: colors.grid, opacity: 0.4 } }
    },
    series
  };
}

const BAND_FLOOR = "band-floor";
const BAND_HEIGHT = "band-height";

/** History shown per forecast bucket, and the floor under it — a 1-bucket
 *  horizon still needs enough past to read a level off. */
const CONTEXT_MULTIPLE = 2;
const MIN_CONTEXT_BUCKETS = 24;

interface ZoomWindow {
  startValue: number;
  endValue: number;
}

/**
 * The slice of the axis to open on, as category indices — or `null` when the
 * whole series already fits.
 *
 * The fit and the view are deliberately different windows. The forecaster
 * refuses a series under eight seasonal cycles, so `projectionRequest` asks for
 * a year of daily history and it has to; but a year rendered into a side panel
 * is ~1.5px per bucket, which turns the actuals into a solid block of noise and
 * leaves the 60 buckets the analyst asked about with a seventh of the width.
 * So: fetch all of it, fit on all of it, *open* on the seam, and leave the rest
 * one drag of the slider away.
 */
function zoomWindow(historyLength: number, forecastLength: number): ZoomWindow | null {
  const context = Math.max(MIN_CONTEXT_BUCKETS, forecastLength * CONTEXT_MULTIPLE);
  if (historyLength <= context) return null;
  return {
    startValue: historyLength - context,
    endValue: historyLength + forecastLength - 1
  };
}

/** Scroll-to-zoom plus a slider. The slider is the visible half — without it
 *  the truncated history is indistinguishable from history that isn't there. */
function dataZoom(window: ZoomWindow, colors: ProjectionChartColors) {
  return [
    { type: "inside" as const, ...window, minValueSpan: 8 },
    {
      type: "slider" as const,
      ...window,
      bottom: 22,
      height: 14,
      showDetail: false,
      brushSelect: false,
      borderColor: colors.grid,
      backgroundColor: "transparent",
      fillerColor: colors.grid,
      handleStyle: { color: colors.axis, borderColor: colors.axis },
      moveHandleStyle: { color: colors.axis, opacity: 0.4 },
      dataBackground: {
        lineStyle: { color: colors.actual, opacity: 0.5 },
        areaStyle: { color: colors.actual, opacity: 0.15 }
      },
      selectedDataBackground: {
        lineStyle: { color: colors.scenario },
        areaStyle: { color: colors.scenario, opacity: 0.2 }
      },
      textStyle: { color: colors.axis, fontSize: 9 }
    }
  ];
}

/**
 * The prediction interval, as a transparent floor plus a shaded height.
 *
 * `null` — no band at all — unless **every** forecast bucket carries a finite
 * pair. A partial band would be drawn as a shape that closes over the buckets
 * it has no bound for, which reads as a narrow interval exactly where the
 * model declined to state one.
 */
function bandSeries(
  projection: MeasureProjection,
  historyLength: number,
  color: string
): LineSeriesOption[] | null {
  const { forecast } = projection;
  if (forecast.length === 0) return null;
  const bounded = forecast.every((p) => typeof p.lower === "number" && typeof p.upper === "number");
  if (!bounded) return null;

  const pad = Array<number | null>(historyLength).fill(null);
  const hidden = { opacity: 0 } as const;
  return [
    {
      name: BAND_FLOOR,
      type: "line",
      stack: "band",
      symbol: "none",
      silent: true,
      lineStyle: hidden,
      areaStyle: { ...hidden, color },
      data: [...pad, ...forecast.map((p) => p.lower as number)]
    },
    {
      name: BAND_HEIGHT,
      type: "line",
      stack: "band",
      symbol: "none",
      silent: true,
      lineStyle: hidden,
      // 0.12 was invisible against a dark card — the interval was drawn and
      // could not be seen, which is the same as not stating it.
      areaStyle: { color, opacity: 0.22 },
      data: [...pad, ...forecast.map((p) => (p.upper as number) - (p.lower as number))]
    }
  ];
}
