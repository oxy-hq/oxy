import type { AsyncDuckDBConnection } from "@duckdb/duckdb-wasm";
import type { EChartsOption } from "echarts";

export interface BaseChartDisplay {
  title?: string;
  data: string;
}

export interface ChartBuilderParams<T extends BaseChartDisplay> {
  display: T;
  connection: AsyncDuckDBConnection;
  fileName: string;
  isDarkMode: boolean;
}

export type ChartOptionsBuilder<T extends BaseChartDisplay> = (
  params: ChartBuilderParams<T>
) => Promise<EChartsOption>;
