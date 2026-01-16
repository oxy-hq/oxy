import { useEffect, useState } from "react";
import type { EChartsOption } from "echarts";
import { DataContainer, TableData } from "@/types/app";
import { getData, registerAuthenticatedFile } from "../utils";
import { getDuckDB } from "@/libs/duckdb";
import useTheme from "@/stores/useTheme";
import type { BaseChartDisplay, ChartOptionsBuilder } from "./types";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

interface UseChartBaseOptions<T extends BaseChartDisplay> {
  display: T;
  data?: DataContainer;
  buildChartOptions: ChartOptionsBuilder<T>;
}

export const useChartBase = <T extends BaseChartDisplay>({
  display,
  data,
  buildChartOptions,
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
        const db = await getDuckDB();
        const fileName = await registerAuthenticatedFile(
          tableData.file_path,
          project.id,
          branchName,
        );
        const connection = await db.connect();

        const options = await buildChartOptions({
          display,
          connection,
          fileName,
          isDarkMode,
        });

        setChartOptions(options);
      } catch (error) {
        console.error("Error loading chart:", error);
        setChartOptions(createErrorOptions(display.title, isDarkMode));
      } finally {
        setIsLoading(false);
      }
    };

    loadChart();
  }, [display, data, isDarkMode, dataAvailable, buildChartOptions]);

  return {
    isLoading,
    chartOptions,
    isDarkMode,
  };
};

const createNoDataOptions = (
  title?: string,
  isDarkMode = false,
): EChartsOption => ({
  darkMode: isDarkMode,
  title: {
    text: title,
    textStyle: {
      color: isDarkMode ? "#ffffff" : "#333333",
    },
  },
  graphic: {
    type: "text",
    left: "center",
    top: "middle",
    style: {
      text: "No data found",
      fontSize: 16,
      fontWeight: "bold",
      fill: isDarkMode ? "#888888" : "#666666",
    },
  },
  xAxis: {
    type: "category",
    show: false,
  },
  yAxis: {
    type: "value",
    show: false,
  },
  series: [],
  grid: {
    containLabel: true,
    show: false,
  },
});

/**
 * Creates standard error state chart options
 */
const createErrorOptions = (
  title?: string,
  isDarkMode = false,
): EChartsOption => ({
  darkMode: isDarkMode,
  title: {
    text: title,
    textStyle: {
      color: isDarkMode ? "#ffffff" : "#333333",
    },
  },
  graphic: {
    type: "text",
    left: "center",
    top: "middle",
    style: {
      text: "Error loading chart",
      fontSize: 16,
      fontWeight: "bold",
      fill: isDarkMode ? "#ff6b6b" : "#e74c3c",
    },
  },
  xAxis: {
    type: "category",
    show: false,
  },
  yAxis: {
    type: "value",
    show: false,
  },
  series: [],
  grid: {
    containLabel: true,
    show: false,
  },
});
