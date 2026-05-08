import type { EChartsOption } from "echarts";
import { resolveColor, resolveColorWithAlpha } from "@/components/Echarts/resolveColor";
import type { DisplayFormat } from "@/types/app";
import { formatValue } from "../utils";

const categoricalPalette = (): string[] => [
  resolveColor("--chart-1"),
  resolveColor("--chart-2"),
  resolveColor("--chart-3"),
  resolveColor("--chart-4"),
  resolveColor("--chart-5")
];

const sequentialPalette = (): string[] => [
  resolveColor("--chart-seq-1"),
  resolveColor("--chart-seq-2"),
  resolveColor("--chart-seq-3"),
  resolveColor("--chart-seq-4"),
  resolveColor("--chart-seq-5")
];

export const createBaseChartOptions = (isDarkMode = false): EChartsOption => {
  const axisColor = resolveColor("--muted-foreground");
  return {
    color: categoricalPalette(),
    darkMode: isDarkMode,
    tooltip: {
      trigger: "axis",
      axisPointer: { type: "shadow" }
    },
    legend: {
      bottom: 0,
      textStyle: { color: axisColor }
    },
    grid: { containLabel: true, bottom: 40 }
  };
};

export const createSingleSeriesPalette = (): string[] => [resolveColor("--chart-primary")];

export const createCategoricalPalette = categoricalPalette;

export const createSequentialPalette = sequentialPalette;

export const createXYAxisOptions = (
  xData: (string | number)[],
  isDarkMode = false,
  yFormat?: DisplayFormat
): Pick<EChartsOption, "xAxis" | "yAxis"> => {
  const axisColor = resolveColor("--muted-foreground");
  const gridColor = isDarkMode
    ? resolveColorWithAlpha("--foreground", 0.1)
    : resolveColorWithAlpha("--muted-foreground", 0.1);
  // Y-axis labels get compact formatting (`$301M`) so monetary scales stay
  // legible when the series runs into the hundreds of millions.
  const yAxisLabelFormatter = yFormat
    ? (value: number) => formatValue(value, yFormat, { compact: true })
    : undefined;
  return {
    xAxis: {
      type: "category",
      data: xData,
      axisLine: { show: false },
      axisTick: { show: false },
      axisLabel: {
        color: axisColor,
        fontSize: 12
      },
      splitLine: {
        lineStyle: {
          color: gridColor,
          type: "dashed"
        }
      }
    },
    yAxis: {
      type: "value",
      axisLine: { show: false },
      axisTick: { show: false },
      axisLabel: {
        color: axisColor,
        fontSize: 12,
        ...(yAxisLabelFormatter ? { formatter: yAxisLabelFormatter } : {})
      },
      splitLine: {
        lineStyle: {
          color: gridColor,
          type: "dashed"
        }
      }
    }
  };
};

/**
 * Tooltip formatter for bar / line charts. Shows the series name + the
 * formatted value (full precision — tooltip isn't space-constrained like
 * the y-axis).
 */
export const createAxisTooltipFormatter = (yFormat?: DisplayFormat) => {
  if (!yFormat) return undefined;
  return (params: unknown) => {
    // ECharts hands us an array of { seriesName, value, axisValueLabel } for
    // axis-trigger tooltips. We render one line per series.
    const items = Array.isArray(params) ? params : [params];
    const header =
      (items[0] as { axisValueLabel?: string; name?: string })?.axisValueLabel ??
      (items[0] as { name?: string })?.name ??
      "";
    const lines = items
      .map((p) => {
        const entry = p as { seriesName?: string; value?: unknown; marker?: string };
        const formatted = formatValue(entry.value, yFormat);
        return `${entry.marker ?? ""} ${entry.seriesName ?? ""}: <b>${formatted}</b>`;
      })
      .join("<br/>");
    return header ? `${header}<br/>${lines}` : lines;
  };
};

export const createPieChartOptions = (
  isDarkMode = false,
  valueFormat?: DisplayFormat,
  sequential = false
): EChartsOption => ({
  ...createBaseChartOptions(isDarkMode),
  color: sequential ? sequentialPalette() : categoricalPalette(),
  tooltip: {
    trigger: "item",
    formatter: (params: unknown) => {
      const entry = params as { name?: string; value?: unknown; percent?: number };
      const formatted = formatValue(entry.value, valueFormat);
      const percent = entry.percent != null ? ` (${entry.percent}%)` : "";
      return `${entry.name ?? ""}: <b>${formatted}</b>${percent}`;
    }
  },
  xAxis: undefined,
  yAxis: undefined
});
