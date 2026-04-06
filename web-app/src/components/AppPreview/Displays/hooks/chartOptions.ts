import type { EChartsOption } from "echarts";
import { resolveColor, resolveColorWithAlpha } from "@/components/Echarts/resolveColor";

export const createBaseChartOptions = (isDarkMode = false): EChartsOption => {
  const axisColor = resolveColor("--muted-foreground");
  return {
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

export const createXYAxisOptions = (
  xData: (string | number)[],
  isDarkMode = false
): Pick<EChartsOption, "xAxis" | "yAxis"> => {
  const axisColor = resolveColor("--muted-foreground");
  const gridColor = isDarkMode
    ? resolveColorWithAlpha("--foreground", 0.1)
    : resolveColorWithAlpha("--muted-foreground", 0.1);
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
        fontSize: 12
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

export const createPieChartOptions = (isDarkMode = false): EChartsOption => ({
  ...createBaseChartOptions(isDarkMode),
  tooltip: {
    trigger: "item",
    formatter: "{b}: {c} ({d}%)"
  },
  xAxis: undefined,
  yAxis: undefined
});
