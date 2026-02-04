// Chart colors
export const CHART_COLORS = {
  success: "#22c55e", // green-500
  error: "#ef4444", // red-500
  info: "#3b82f6", // blue-500
  warning: "#f97316" // orange-500
} as const;

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
export const AXIS_LABEL_STYLE = {
  show: true,
  fontSize: 9,
  color: "#888"
} as const;
