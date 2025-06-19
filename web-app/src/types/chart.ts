export type ChartType = "line" | "bar" | "pie";

export interface AxisConfig {
  type: string;
  name?: string;
  data?: (string | number | Date)[];
}

export interface SeriesConfig {
  name?: string;
  type: ChartType;
  data?: (number | { name: string; value: number })[];
}

export interface ChartConfig {
  xAxis?: AxisConfig;
  yAxis?: AxisConfig;
  series: SeriesConfig[];
  title?: string;
}
