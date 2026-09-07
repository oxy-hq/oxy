import type { EChartsOption } from "echarts";
import type { SimulationFit, SimulationPeriod } from "@/types/simulation";

/**
 * Chart options for the two things a run has to show.
 *
 * Kept out of the components so the shaping rules — which are the load-bearing
 * part — are testable without rendering anything.
 */

/** Semantic colours. Emerald is reserved for workflow-node success, so it is
 *  deliberately absent here even though "converged" is the good outcome. */
const TRUTH = "#94a3b8";
const ESTIMATE = "#3b82f6";
const BAND = "rgba(59, 130, 246, 0.15)";

const AXIS_FONT = 10;

/**
 * These charts live in the Metric Tree's side panel, which is ~340px wide at
 * its minimum — a third of what the echarts defaults assume.
 *
 * `containLabel` is what makes that survivable: the plot area shrinks to fit
 * its own tick text instead of the fixed `left` gutter being either too small
 * (clipped labels) or too large (a plot squeezed to nothing). The top strip is
 * reserved for the legend.
 */
function panelGrid() {
  return { left: 4, right: 8, top: 28, bottom: 4, containLabel: true };
}

/**
 * Legend pinned top-LEFT, and that side matters: the toolbox (zoom / restore /
 * save) sits top-right by default, so a centred legend — the echarts default —
 * lands under those icons in a panel this narrow.
 */
function panelLegend(data: string[], isDark: boolean) {
  return {
    data,
    top: 0,
    left: 0,
    itemGap: 12,
    itemWidth: 14,
    itemHeight: 8,
    textStyle: { color: isDark ? "#cbd5e1" : "#334155", fontSize: AXIS_FONT }
  };
}

/** Period axis. No `name`: an axis name renders at the axis end, where it
 *  collides with the last tick label once the plot is only ~250px wide — and
 *  the stepper above the chart already says which period is which. */
function periodAxis(data: (number | string)[]) {
  return {
    type: "category" as const,
    data,
    boundaryGap: false,
    axisTick: { show: false },
    // 30 periods is more ticks than a narrow panel can letter; echarts drops
    // the ones that would overlap rather than drawing them on top of each other.
    axisLabel: { fontSize: AXIS_FONT, hideOverlap: true }
  };
}

const COMPACT = new Intl.NumberFormat(undefined, {
  notation: "compact",
  maximumFractionDigits: 1
});

/** `3,500,000` as a tick label costs ~70px of a ~340px panel. `3.5M` costs 24. */
function compactAxisLabel(value: number): string {
  return COMPACT.format(value);
}

/**
 * β̂ with its ±2·se band, against the true line.
 *
 * A **refused** period is a gap, not a point. `coefficient` is null exactly on a
 * refusal, and echarts renders null as a break in the line — which is the honest
 * rendering: the model declined, it did not estimate zero. Plotting refusals as
 * zeros would draw a collapse that never happened.
 */
export function convergenceOptions(fits: SimulationFit[], isDark: boolean): EChartsOption {
  const periods = fits.map((f) => f.period);
  const estimate = fits.map((f) => f.coefficient);
  const truth = fits.map((f) => f.true_local_slope);
  // Stacked band: lower bound, then the width above it. Only drawn where the
  // fit produced both a coefficient and an se.
  const lower = fits.map((f) =>
    f.coefficient !== null && f.se !== null ? f.coefficient - 2 * f.se : null
  );
  const width = fits.map((f) => (f.se !== null ? 4 * f.se : null));

  return {
    grid: panelGrid(),
    tooltip: { trigger: "axis" },
    legend: panelLegend(["β̂", "β true"], isDark),
    xAxis: periodAxis(periods),
    yAxis: {
      type: "value",
      scale: true,
      axisLabel: { fontSize: AXIS_FONT, formatter: compactAxisLabel }
    },
    series: [
      {
        name: "ci-lower",
        type: "line",
        data: lower,
        stack: "ci",
        lineStyle: { opacity: 0 },
        showSymbol: false,
        silent: true,
        tooltip: { show: false }
      },
      {
        name: "ci",
        type: "line",
        data: width,
        stack: "ci",
        lineStyle: { opacity: 0 },
        areaStyle: { color: BAND },
        showSymbol: false,
        silent: true,
        tooltip: { show: false }
      },
      {
        name: "β̂",
        type: "line",
        data: estimate,
        connectNulls: false,
        // Small, but drawn: a period whose neighbours were both refused is a
        // lone point with no line to hang on, so symbols cannot be dropped.
        symbolSize: 4,
        lineStyle: { color: ESTIMATE, width: 2 },
        itemStyle: { color: ESTIMATE }
      },
      {
        name: "β true",
        type: "line",
        data: truth,
        showSymbol: false,
        lineStyle: { color: TRUTH, width: 2, type: "dashed" },
        itemStyle: { color: TRUTH }
      }
    ]
  };
}

/**
 * Cumulative profit over the run.
 *
 * One line per run today. The plan's "profit race" compares policies, and the
 * arms are now runs of ONE world on one seed (`?policies=hold,machine`), so
 * racing them is a matter of passing several — which is
 * why this takes a list rather than one series.
 */
export function profitRaceOptions(
  series: { label: string; periods: SimulationPeriod[] }[],
  isDark: boolean
): EChartsOption {
  const longest = series.reduce((n, s) => Math.max(n, s.periods.length), 0);
  return {
    grid: panelGrid(),
    tooltip: { trigger: "axis" },
    legend: panelLegend(
      series.map((s) => s.label),
      isDark
    ),
    xAxis: periodAxis(Array.from({ length: longest }, (_, i) => i + 1)),
    // No axis name here either — "cumulative profit" is the section subtitle,
    // and as a y-axis name it printed above the tick labels where it read as a
    // second heading for the chart.
    yAxis: {
      type: "value",
      scale: true,
      axisLabel: { fontSize: AXIS_FONT, formatter: compactAxisLabel }
    },
    series: series.map((s) => ({
      name: s.label,
      type: "line" as const,
      data: s.periods.map((p) => p.cumulative_profit),
      showSymbol: false,
      lineStyle: { width: 2 }
    }))
  };
}
