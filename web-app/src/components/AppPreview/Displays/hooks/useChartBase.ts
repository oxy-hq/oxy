import type { EChartsOption } from "echarts";
import { useEffect, useState } from "react";
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
  const [isLoading, setIsLoading] = useState(true);
  const { project, branchName } = useCurrentProjectBranch();
  const [chartOptions, setChartOptions] = useState<EChartsOption>({});
  const { theme } = useTheme();
  const isDarkMode = theme === "dark";

  const dataAvailable = data && display.data;

  useEffect(() => {
    const loadChart = async () => {
      setIsLoading(true);

      if (!dataAvailable) {
        setChartOptions(createNoDataOptions(display.title, isDarkMode));
        setIsLoading(false);
        return;
      }

      try {
        const tableData = getData(data, display.data) as TableData | null;
        if (!tableData) {
          setChartOptions(createNoDataOptions(display.title, isDarkMode));
          setIsLoading(false);
          return;
        }
        // Empty JSON result (e.g. date filter returns 0 rows) — show "No data"
        // instead of trying to register an empty array in DuckDB, which fails.
        if (typeof tableData.json === "string" && tableData.json.trim() === "[]") {
          setChartOptions(createNoDataOptions(display.title, isDarkMode));
          setIsLoading(false);
          return;
        }
        const fileName = await registerFromTableData(tableData, project.id, branchName);
        const db = await getDuckDB();
        const connection = await db.connect();

        try {
          const options = await buildChartOptions({
            display,
            connection,
            fileName,
            isDarkMode
          });
          setChartOptions(options);
        } finally {
          await connection.close();
        }
      } catch (error) {
        console.error("Error loading chart:", error);
        setChartOptions(createErrorOptions(display.title, isDarkMode));
      } finally {
        setIsLoading(false);
      }
    };

    loadChart();
  }, [display, data, isDarkMode, dataAvailable, buildChartOptions, branchName, project.id]);

  return {
    isLoading,
    chartOptions,
    isDarkMode
  };
};

const createNoDataOptions = (title?: string, isDarkMode = false): EChartsOption => ({
  darkMode: isDarkMode,
  title: title
    ? {
        text: title,
        textStyle: {
          color: isDarkMode ? "#f3f4f6" : "#111827",
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
        fill: isDarkMode ? "#6b7280" : "#9ca3af"
      }
    }
  ],
  xAxis: { type: "category", show: false },
  yAxis: { type: "value", show: false },
  series: [],
  grid: { containLabel: true, show: false }
});

const createErrorOptions = (title?: string, isDarkMode = false): EChartsOption => ({
  darkMode: isDarkMode,
  title: title
    ? {
        text: title,
        textStyle: {
          color: isDarkMode ? "#f3f4f6" : "#111827",
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
        fill: "#f43f5e"
      }
    }
  ],
  xAxis: { type: "category", show: false },
  yAxis: { type: "value", show: false },
  series: [],
  grid: { containLabel: true, show: false }
});
