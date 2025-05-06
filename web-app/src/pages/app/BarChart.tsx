import { useEffect, useState } from "react";
import type { BarSeriesOption } from "echarts";
import type { EChartsOption } from "echarts";
import { BarChartDisplay, DataContainer, TableData } from "@/types/app";
import { getArrowValue, getData, getDataFileUrl } from "./utils";
import { Echarts } from "@/components/Echarts";
import { getDuckDB } from "@/libs/duckdb";
import useTheme from "@/stores/useTheme";

export const BarChart = ({
  display,
  data,
}: {
  display: BarChartDisplay;
  data: DataContainer;
}) => {
  const value = getData(data, display.data) as TableData;
  const [isLoading, setIsLoading] = useState(true);
  const { theme } = useTheme();
  const isDarkMode = theme === "dark";
  const [chartOptions, setChartOptions] = useState<EChartsOption>({});

  useEffect(() => {
    (async (): Promise<void> => {
      const db = await getDuckDB();
      const file_name = `${btoa(value.file_path)}.parquet`;
      const conn = await db.connect();
      await db.registerFileURL(
        file_name,
        getDataFileUrl(value.file_path),
        4,
        true,
      );

      const options: EChartsOption = {
        darkMode: isDarkMode,
        title: { text: display.title },
        tooltip: {},
        xAxis: { type: "category" },
        yAxis: { type: "value" },
        series: [],
        grid: { containLabel: true },
      };

      const xData = await conn.query(
        `select distinct ${display.x} as x from "${file_name}"`,
      );
      options.xAxis = {
        ...options.xAxis,
        data: xData.toArray().map((row) => getArrowValue(row.x)) as (
          | string
          | number
        )[],
      };

      if (display.series) {
        const seriesStmt = await conn.prepare(
          `SELECT DISTINCT ${display.series} as series from "${file_name}";`,
        );
        const series = await seriesStmt.query();
        const seriesDataStatement = await conn.prepare(
          `SELECT ${display.x} as x, SUM(${display.y}) as y from "${file_name}" where ${display.series} = ? group by ${display.x}, ${display.series};`,
        );
        const seriesData: BarSeriesOption[] = [];
        for (const seriesItem of series.toArray().map((row) => row.series)) {
          const yData = await seriesDataStatement.query(seriesItem);
          seriesData.push({
            name: seriesItem,
            type: "bar",
            stack: "total",
            data: yData.toArray().map((row) => getArrowValue(row.y)) as (
              | number
              | string
            )[],
          });
        }
        options.series = seriesData;
      } else {
        const yData = await conn.query(
          `SELECT ${display.x} as x, SUM(${display.y}) as y from "${file_name}" group by ${display.x};`,
        );
        options.series = [
          {
            name: display.y,
            type: "bar",
            data: yData.toArray().map((row) => getArrowValue(row.y)) as (
              | string
              | number
            )[],
          },
        ];
      }
      setChartOptions(options);
      setIsLoading(false);
    })();
  }, [display, value.file_path, data, isDarkMode]);

  if (!chartOptions)
    return (
      <div className="w-full h-full flex items-center justify-center">
        Loading...
      </div>
    );
  return <Echarts isLoading={isLoading} options={chartOptions} />;
};
