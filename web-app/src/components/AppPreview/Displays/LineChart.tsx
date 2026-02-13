import type { LineSeriesOption } from "echarts";
import { useCallback } from "react";
import { Echarts } from "@/components/Echarts";
import type { DataContainer, LineChartDisplay } from "@/types/app";
import {
  type ChartBuilderParams,
  createBaseChartOptions,
  createXYAxisOptions,
  getSeriesData,
  getSeriesValues,
  getSimpleAggregatedData,
  getXAxisData,
  useChartBase
} from "./hooks";

export const LineChart = ({
  display,
  data,
  index
}: {
  display: LineChartDisplay;
  data?: DataContainer;
  index?: number;
}) => {
  const buildChartOptions = useCallback(
    async ({ display, connection, fileName, isDarkMode }: ChartBuilderParams<LineChartDisplay>) => {
      const baseOptions = createBaseChartOptions(isDarkMode);
      const xData = await getXAxisData(connection, fileName, display.x);
      const xyAxisOptions = createXYAxisOptions(xData, isDarkMode);

      // Configure tooltip to show values on hover
      const tooltipOptions = {
        trigger: "axis" as const,
        axisPointer: {
          type: "line" as const
        }
      };

      let series: LineSeriesOption[];

      if (display.series) {
        const seriesNames = await getSeriesData(connection, fileName, display.series);
        series = await Promise.all(
          seriesNames.map(async (seriesName): Promise<LineSeriesOption> => {
            const values = await getSeriesValues(
              connection,
              fileName,
              display.x,
              display.y,
              display.series!,
              seriesName
            );
            // Create a map of x -> y for this series
            const valueMap = new Map(values.map((v) => [v.x, v.y]));
            // Align data with xData axis, using null for missing values
            const alignedData = xData.map((x) => valueMap.get(x) ?? null);
            return {
              name: JSON.stringify(seriesName),
              type: "line",
              data: alignedData,
              showSymbol: false
            };
          })
        );
      } else {
        const values = await getSimpleAggregatedData(connection, fileName, display.x, display.y);
        series = [
          {
            name: display.y,
            type: "line",
            data: values,
            showSymbol: false
          }
        ];
      }

      return {
        ...baseOptions,
        ...xyAxisOptions,
        tooltip: tooltipOptions,
        series
      };
    },
    []
  );

  const { isLoading, chartOptions } = useChartBase({
    display,
    data,
    buildChartOptions
  });

  return (
    <Echarts
      options={chartOptions}
      isLoading={isLoading}
      title={display.title}
      testId='app-line-chart'
      chartIndex={index}
    />
  );
};
