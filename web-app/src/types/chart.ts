type ChartType = "line" | "bar" | "pie";

interface AxisConfig {
  type: string;
  name?: string;
  data?: (string | number | Date)[];
}

interface SeriesConfig {
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
