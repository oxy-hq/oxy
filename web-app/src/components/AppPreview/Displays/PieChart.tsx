import { useEffect, useState } from "react";
import type { PieSeriesOption } from "echarts";
import type { EChartsOption } from "echarts";
import { DataContainer, PieChartDisplay, TableData } from "@/types/app";
import { getArrowValue, getData, getDataFileUrl } from "./utils";
import { Echarts } from "@/components/Echarts";
import { getDuckDB } from "@/libs/duckdb";
import useTheme from "@/stores/useTheme";

export const PieChart = ({
  display,
  data,
}: {
  display: PieChartDisplay;
  data: DataContainer;
}) => {
  const value = getData(data, display.data) as TableData;
  const [isLoading, setIsLoading] = useState(true);
  const [chartOptions, setChartOptions] = useState<EChartsOption>({});
  const { theme } = useTheme();
  const isDarkMode = theme === "dark";

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
        tooltip: {
          trigger: "item",
          formatter: "{b}: {c} ({d}%)",
        },
        series: [],
        grid: { containLabel: true },
      };

      const pieData = await conn.query(
        `select ${display.name} as name,sum(${display.value}) as value from "${file_name}" group by ${display.name};`,
      );
      const pieSeries: PieSeriesOption = {
        type: "pie",
        data: pieData
          .toArray()
          .map((row) => ({
            name: getArrowValue(row.name),
            value: getArrowValue(row.value),
          }))
          .filter((row) => row.name && row.value) as {
          name: string;
          value: number;
        }[],
        emphasis: {
          itemStyle: {
            shadowBlur: 10,
            shadowOffsetX: 0,
            shadowColor: "rgba(0, 0, 0, 0.5)",
          },
        },
      };
      options.series = [pieSeries];
      setChartOptions(options);
      setIsLoading(false);
    })();
  }, [display, value.file_path, data, isDarkMode]);

  return <Echarts isLoading={isLoading} options={chartOptions} />;
};
