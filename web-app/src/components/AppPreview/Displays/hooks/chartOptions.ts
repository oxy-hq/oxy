import type { EChartsOption } from "echarts";

export const createBaseChartOptions = (isDarkMode = false): EChartsOption => ({
  darkMode: isDarkMode,
  tooltip: {},
  grid: { containLabel: true }
});

export const createXYAxisOptions = (
  xData: (string | number)[],
  isDarkMode = false
): Pick<EChartsOption, "xAxis" | "yAxis"> => ({
  xAxis: {
    type: "category",
    data: xData,
    axisLabel: {
      color: isDarkMode ? "#cccccc" : "#666666"
    }
  },
  yAxis: {
    type: "value",
    axisLabel: {
      color: isDarkMode ? "#cccccc" : "#666666"
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
