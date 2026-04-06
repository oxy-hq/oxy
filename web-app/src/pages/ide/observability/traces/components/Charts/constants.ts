import { resolveColor } from "@/components/Echarts/resolveColor";

// Chart colors — resolved from CSS variables for ECharts canvas
export const getChartColors = () => ({
  success: resolveColor("--success"),
  error: resolveColor("--error"),
  info: resolveColor("--info"),
  warning: resolveColor("--warning")
});

// Common axis styling for mini charts
export const AXIS_STYLE = {
  axisLabel: { show: false },
  axisTick: { show: false },
  axisLine: { show: false },
  splitLine: { show: false }
} as const;

// Chart grid configuration
export const CHART_GRID = {
  top: 5,
  right: 5,
  bottom: 20,
  left: 5
} as const;

// Axis label style
export const getAxisLabelStyle = () =>
  ({
    show: true,
    fontSize: 9,
    color: resolveColor("--muted-foreground")
  }) as const;
