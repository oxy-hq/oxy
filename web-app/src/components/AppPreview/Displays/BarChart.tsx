import { useCallback } from "react";
import type { BarSeriesOption } from "echarts";
import { BarChartDisplay, DataContainer } from "@/types/app";
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

export const BarChart = ({
  display,
  data,
}: {
  display: BarChartDisplay;
  data?: DataContainer;
}) => {
  const buildChartOptions = useCallback(
    async ({
      display,
      connection,
      fileName,
      isDarkMode,
    }: ChartBuilderParams<BarChartDisplay>) => {
      const baseOptions = createBaseChartOptions(display.title, isDarkMode);
      const xData = await getXAxisData(connection, fileName, display.x);
      const xyAxisOptions = createXYAxisOptions(xData, isDarkMode);

      let series: BarSeriesOption[];

      if (display.series) {
        const seriesNames = await getSeriesData(
          connection,
          fileName,
          display.series,
        );
        series = await Promise.all(
          seriesNames.map(async (seriesName): Promise<BarSeriesOption> => {
            const values = await getSeriesValues(
              connection,
              fileName,
              display.x,
              display.y,
              display.series!,
              seriesName,
            );
            return {
              name: String(seriesName),
              type: "bar",
              stack: "total",
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
            type: "bar",
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

  return <Echarts isLoading={isLoading} options={chartOptions} />;
};
