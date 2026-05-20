import { useQuery } from "@tanstack/react-query";
import type { EChartsOption } from "echarts";
import { resolveColor } from "@/components/Echarts/resolveColor";
import queryKeys from "@/hooks/api/queryKey";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { getDuckDB } from "@/libs/duckdb";
import useTheme from "@/stores/useTheme";
import type { DataContainer, TableData } from "@/types/app";
import { getData, registerFromTableData } from "../utils";
import type { BaseChartDisplay, ChartOptionsBuilder } from "./types";

interface UseChartBaseOptions<T extends BaseChartDisplay> {
  display: T;
  data?: DataContainer;
  buildChartOptions: ChartOptionsBuilder<T>;
}

export const useChartBase = <T extends BaseChartDisplay>({
  display,
  data,
  buildChartOptions
}: UseChartBaseOptions<T>) => {
  const { project, branchName } = useCurrentProjectBranch();
  const { theme } = useTheme();
  const isDarkMode = theme === "dark";
  const dataAvailable = data && display.data;

  // Resolve the referenced data slice up-front so we can put a *stable*
  // identifier (the parquet file_path) into the query key instead of the
  // whole DataContainer blob — otherwise every render re-hashes a payload
  // that can be hundreds of KB.
  const referencedTable = dataAvailable
    ? ((getData(data, display.data) as TableData | null) ?? null)
    : null;
  const tableKey = referencedTable?.file_path ?? "no-data";
  const displayKey = JSON.stringify(display);

  const {
    isPending,
    isError,
    data: chartOptions
  } = useQuery({
    queryKey: queryKeys.chart.fromDisplay(project.id, branchName, displayKey, tableKey, isDarkMode),
    queryFn: async () => {
      if (!dataAvailable) {
        return createNoDataOptions(display.title);
      }

      const tableData = referencedTable;
      if (!tableData) {
        return createNoDataOptions(display.title);
      }

      // Empty JSON result (e.g. date filter returns 0 rows) — show "No data"
      // instead of trying to register an empty array in DuckDB, which fails.
      if (typeof tableData.json === "string" && tableData.json.trim() === "[]") {
        return createNoDataOptions(display.title);
      }

      const fileName = await registerFromTableData(tableData, project.id, branchName);
      const db = await getDuckDB();
      const connection = await db.connect();

      try {
        return await buildChartOptions({ display, connection, fileName, isDarkMode });
      } finally {
        await connection.close();
      }
    },
    retry: false
  });

  return {
    isLoading: isPending,
    chartOptions: isError ? createErrorOptions(display.title) : (chartOptions ?? {}),
    isDarkMode
  };
};

const createNoDataOptions = (title?: string): EChartsOption => ({
  title: title
    ? {
        text: title,
        textStyle: {
          color: resolveColor("--foreground"),
          fontSize: 16,
          fontWeight: "bold"
        }
      }
    : undefined,
  graphic: [
    {
      type: "text",
      left: "center",
      top: "middle",
      style: {
        text: "No data found",
        fontSize: 14,
        fill: resolveColor("--muted-foreground")
      }
    }
  ],
  xAxis: { type: "category", show: false },
  yAxis: { type: "value", show: false },
  series: [],
  grid: { containLabel: true, show: false }
});

const createErrorOptions = (title?: string): EChartsOption => ({
  title: title
    ? {
        text: title,
        textStyle: {
          color: resolveColor("--foreground"),
          fontSize: 16,
          fontWeight: "bold"
        }
      }
    : undefined,
  graphic: [
    {
      type: "text",
      left: "center",
      top: "middle",
      style: {
        text: "Error loading chart",
        fontSize: 14,
        fill: resolveColor("--destructive")
      }
    }
  ],
  xAxis: { type: "category", show: false },
  yAxis: { type: "value", show: false },
  series: [],
  grid: { containLabel: true, show: false }
});
