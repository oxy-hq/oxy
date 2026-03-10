import type { EChartsOption } from "echarts";

const AXIS_COLOR_DARK = "#9ca3af"; // gray-400 — readable on dark backgrounds
const AXIS_COLOR_LIGHT = "#6b7280"; // gray-500 — readable on light backgrounds
const GRID_COLOR_DARK = "rgba(255,255,255,0.08)";
const GRID_COLOR_LIGHT = "rgba(107,114,128,0.15)";

export const createBaseChartOptions = (isDarkMode = false): EChartsOption => ({
  darkMode: isDarkMode,
  tooltip: {
    trigger: "axis",
    axisPointer: { type: "shadow" }
  },
  legend: {
    bottom: 0,
    textStyle: { color: isDarkMode ? AXIS_COLOR_DARK : AXIS_COLOR_LIGHT }
  },
  grid: { containLabel: true, bottom: 40 }
});

export const createXYAxisOptions = (
  xData: (string | number)[],
  isDarkMode = false
): Pick<EChartsOption, "xAxis" | "yAxis"> => ({
  xAxis: {
    type: "category",
    data: xData,
    axisLine: { show: false },
    axisTick: { show: false },
    axisLabel: {
      color: isDarkMode ? AXIS_COLOR_DARK : AXIS_COLOR_LIGHT,
      fontSize: 12
    },
    splitLine: {
      lineStyle: {
        color: isDarkMode ? GRID_COLOR_DARK : GRID_COLOR_LIGHT,
        type: "dashed"
      }
    }
  },
  yAxis: {
    type: "value",
    axisLine: { show: false },
    axisTick: { show: false },
    axisLabel: {
      color: isDarkMode ? AXIS_COLOR_DARK : AXIS_COLOR_LIGHT,
      fontSize: 12
    },
    splitLine: {
      lineStyle: {
        color: isDarkMode ? GRID_COLOR_DARK : GRID_COLOR_LIGHT,
        type: "dashed"
      }
    }
  }
});

export const createPieChartOptions = (isDarkMode = false): EChartsOption => ({
  ...createBaseChartOptions(isDarkMode),
  tooltip: {
    trigger: "item",
    formatter: "{b}: {c} ({d}%)"
  },
  xAxis: undefined,
  yAxis: undefined
});
