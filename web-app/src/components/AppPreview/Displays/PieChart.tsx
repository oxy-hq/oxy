import { useCallback } from "react";
import type { PieSeriesOption } from "echarts";
import { DataContainer, PieChartDisplay } from "@/types/app";
import { Echarts } from "@/components/Echarts";
import {
  useChartBase,
  getPieChartData,
  createPieChartOptions,
  type ChartBuilderParams,
} from "./hooks";

export const PieChart = ({
  display,
  data,
}: {
  display: PieChartDisplay;
  data?: DataContainer;
}) => {
  const buildChartOptions = useCallback(
    async ({
      display,
      connection,
      fileName,
      isDarkMode,
    }: ChartBuilderParams<PieChartDisplay>) => {
      const baseOptions = createPieChartOptions(isDarkMode);

      const pieData = await getPieChartData(
        connection,
        fileName,
        display.name,
        display.value,
      );

      const pieSeries: PieSeriesOption = {
        type: "pie",
        data: pieData,
        emphasis: {
          itemStyle: {
            shadowBlur: 10,
            shadowOffsetX: 0,
            shadowColor: "rgba(0, 0, 0, 0.5)",
          },
        },
      };

      return {
        ...baseOptions,
        series: [pieSeries],
      };
    },
    [],
  );

  const { isLoading, chartOptions } = useChartBase({
    display,
    data,
    buildChartOptions,
  });

  return (
    <Echarts
      isLoading={isLoading}
      options={chartOptions}
      title={display.title}
    />
  );
};
