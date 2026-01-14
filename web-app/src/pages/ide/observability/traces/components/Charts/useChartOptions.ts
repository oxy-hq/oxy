import { useMemo } from "react";
import type { EChartsOption } from "echarts";
import { TimeBucket, DurationBucket } from "./types";
import {
  CHART_COLORS,
  AXIS_STYLE,
  CHART_GRID,
  AXIS_LABEL_STYLE,
} from "./constants";

type TooltipFormatter = (params: unknown) => string;

function createTooltipFormatter(
  label: string,
  formatValue?: (v: number) => string,
): TooltipFormatter {
  return (params: unknown) => {
    const data = (params as Array<{ name: string; value: number }>)[0];
    const value = formatValue ? formatValue(data.value) : data.value;
    return `${data.name}<br/>${label}: ${value}`;
  };
}

export function useAgentRunsChartOptions(
  timeBuckets: TimeBucket[],
): EChartsOption {
  return useMemo(
    () => ({
      tooltip: {
        trigger: "axis",
        formatter: createTooltipFormatter("Agent Runs"),
      },
      grid: CHART_GRID,
      xAxis: {
        type: "category",
        data: timeBuckets.map((b) => b.time),
        ...AXIS_STYLE,
        axisLabel: AXIS_LABEL_STYLE,
      },
      yAxis: {
        type: "value",
        ...AXIS_STYLE,
      },
      series: [
        {
          type: "bar",
          data: timeBuckets.map((b) => b.agentCount),
          itemStyle: { color: CHART_COLORS.success },
          barMaxWidth: 20,
        },
      ],
    }),
    [timeBuckets],
  );
}

export function useWorkflowRunsChartOptions(
  timeBuckets: TimeBucket[],
): EChartsOption {
  return useMemo(
    () => ({
      tooltip: {
        trigger: "axis",
        formatter: createTooltipFormatter("Workflow Runs"),
      },
      grid: CHART_GRID,
      xAxis: {
        type: "category",
        data: timeBuckets.map((b) => b.time),
        ...AXIS_STYLE,
        axisLabel: AXIS_LABEL_STYLE,
      },
      yAxis: {
        type: "value",
        ...AXIS_STYLE,
      },
      series: [
        {
          type: "bar",
          data: timeBuckets.map((b) => b.workflowCount),
          itemStyle: { color: CHART_COLORS.info },
          barMaxWidth: 20,
        },
      ],
    }),
    [timeBuckets],
  );
}

export function useDurationChartOptions(
  durationBuckets: DurationBucket[],
): EChartsOption {
  return useMemo(
    () => ({
      tooltip: {
        trigger: "axis",
        formatter: createTooltipFormatter("Count"),
      },
      grid: CHART_GRID,
      xAxis: {
        type: "category",
        data: durationBuckets.map((b) => b.range),
        ...AXIS_STYLE,
        axisLabel: { ...AXIS_LABEL_STYLE, rotate: 0 },
      },
      yAxis: {
        type: "value",
        ...AXIS_STYLE,
      },
      series: [
        {
          type: "bar",
          data: durationBuckets.map((b) => b.count),
          itemStyle: { color: CHART_COLORS.info },
          barMaxWidth: 30,
        },
      ],
    }),
    [durationBuckets],
  );
}

export function useTokensChartOptions(
  timeBuckets: TimeBucket[],
): EChartsOption {
  return useMemo(
    () => ({
      tooltip: {
        trigger: "axis",
        formatter: createTooltipFormatter("Tokens", (v) => v.toLocaleString()),
      },
      grid: CHART_GRID,
      xAxis: {
        type: "category",
        data: timeBuckets.map((b) => b.time),
        ...AXIS_STYLE,
        axisLabel: AXIS_LABEL_STYLE,
      },
      yAxis: {
        type: "value",
        ...AXIS_STYLE,
      },
      series: [
        {
          type: "bar",
          data: timeBuckets.map((b) => b.tokens),
          itemStyle: { color: CHART_COLORS.warning },
          barMaxWidth: 20,
        },
      ],
    }),
    [timeBuckets],
  );
}
