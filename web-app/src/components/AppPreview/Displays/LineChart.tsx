import { useEffect, useState } from "react";
import type { EChartsOption, LineSeriesOption } from "echarts";
import { DataContainer, LineChartDisplay, TableData } from "@/types/app";
import { getArrowValue, getData, getDataFileUrl } from "./utils";
import { Echarts } from "@/components/Echarts";
import { getDuckDB } from "@/libs/duckdb";
import useTheme from "@/stores/useTheme";

export const LineChart = ({
  display,
  data,
}: {
  display: LineChartDisplay;
  data: DataContainer;
}) => {
  const dt = getData(data, display.data) as unknown as TableData;
  const [isLoading, setIsLoading] = useState(true);
  const [chartOptions, setChartOptions] = useState<EChartsOption>({});
  const { theme } = useTheme();
  const isDarkMode = theme === "dark";

  useEffect(() => {
    (async () => {
      const db = await getDuckDB();
      const file_name = `${btoa(dt.file_path)}.parquet`;
      const conn = await db.connect();
      await db.registerFileURL(
        file_name,
        getDataFileUrl(dt.file_path),
        4,
        true,
      );

      let options: EChartsOption = {
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
        const seriesData: LineSeriesOption[] = [];
        for (const seriesItem of series
          .toArray()
          .map((row) => getArrowValue(row.series))) {
          const yData = await seriesDataStatement.query(seriesItem);
          seriesData.push({
            name: JSON.stringify(seriesItem),
            type: "line",
            data: yData.toArray().map((row) => getArrowValue(row.y)) as (
              | number
              | string
            )[],
          });
        }
        options = { ...options, series: seriesData };
      } else {
        const yData = await conn.query(
          `SELECT ${display.x} as x, SUM(${display.y}) as y from "${file_name}" group by ${display.x};`,
        );
        const dt = yData.toArray().map((row) => {
          return getArrowValue(row.y);
        });
        options.series = [
          {
            name: display.y,
            type: "line",
            data: dt,
          } as LineSeriesOption,
        ];
      }
      setChartOptions(options);
      setIsLoading(false);
    })();
  }, [display, dt.file_path, isDarkMode]);

  return <Echarts options={chartOptions} isLoading={isLoading} />;
};
