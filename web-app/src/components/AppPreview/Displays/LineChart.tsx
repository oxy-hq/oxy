import { useCallback } from "react";
import type { LineSeriesOption } from "echarts";
import { DataContainer, LineChartDisplay } from "@/types/app";
import { Echarts } from "@/components/Echarts";
import {
  useChartBase,
  getXAxisData,
  getSeriesData,
  getSeriesValues,
  getSimpleAggregatedData,
  createBaseChartOptions,
  createXYAxisOptions,
  type ChartBuilderParams,
} from "./hooks";

export const LineChart = ({
  display,
  data,
}: {
  display: LineChartDisplay;
  data?: DataContainer;
}) => {
  const buildChartOptions = useCallback(
    async ({
      display,
      connection,
      fileName,
      isDarkMode,
    }: ChartBuilderParams<LineChartDisplay>) => {
      const baseOptions = createBaseChartOptions(display.title, isDarkMode);
      const xData = await getXAxisData(connection, fileName, display.x);
      const xyAxisOptions = createXYAxisOptions(xData, isDarkMode);

      let series: LineSeriesOption[];

      if (display.series) {
        const seriesNames = await getSeriesData(
          connection,
          fileName,
          display.series,
        );
        series = await Promise.all(
          seriesNames.map(async (seriesName): Promise<LineSeriesOption> => {
            const values = await getSeriesValues(
              connection,
              fileName,
              display.x,
              display.y,
              display.series!,
              seriesName,
            );
            return {
              name: JSON.stringify(seriesName),
              type: "line",
              data: values,
            };
          }),
        );
      } else {
        const values = await getSimpleAggregatedData(
          connection,
          fileName,
          display.x,
          display.y,
        );
        series = [
          {
            name: display.y,
            type: "line",
            data: values,
          },
        ];
      }

      return {
        ...baseOptions,
        ...xyAxisOptions,
        series,
      };
    },
    [],
  );

  const { isLoading, chartOptions } = useChartBase({
    display,
    data,
    buildChartOptions,
  });

  return <Echarts options={chartOptions} isLoading={isLoading} />;
};
