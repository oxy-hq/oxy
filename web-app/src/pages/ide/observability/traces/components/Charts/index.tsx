import { useMemo } from "react";
import { TraceChartsProps } from "./types";
import { aggregateByTime, aggregateByDuration, calculateStats } from "./utils";
import {
  useAgentRunsChartOptions,
  useWorkflowRunsChartOptions,
  useDurationChartOptions,
  useTokensChartOptions,
} from "./useChartOptions";
import ChartCard from "./ChartCard";

export default function TraceCharts({ traces, isLoading }: TraceChartsProps) {
  const timeBuckets = useMemo(() => aggregateByTime(traces ?? []), [traces]);

  const durationBuckets = useMemo(
    () => aggregateByDuration(traces ?? []),
    [traces],
  );

  const stats = useMemo(() => calculateStats(traces), [traces]);

  const agentRunsChartOptions = useAgentRunsChartOptions(timeBuckets);
  const workflowRunsChartOptions = useWorkflowRunsChartOptions(timeBuckets);
  const durationChartOptions = useDurationChartOptions(durationBuckets);
  const tokensChartOptions = useTokensChartOptions(timeBuckets);

  return (
    <div className="grid grid-cols-4 gap-4 mb-4">
      <ChartCard
        title="Agent Runs"
        value={`${stats.agentRuns} Agent Runs`}
        subtitle=""
        options={agentRunsChartOptions}
        isLoading={isLoading}
      />

      <ChartCard
        title="Workflow Runs"
        value={`${stats.workflowRuns} Workflow Runs`}
        subtitle=""
        options={workflowRunsChartOptions}
        isLoading={isLoading}
      />

      <ChartCard
        title="Duration"
        value={`${stats.avgDuration} Average Execution Time`}
        subtitle=""
        options={durationChartOptions}
        isLoading={isLoading}
      />

      <ChartCard
        title="Tokens"
        value={`${stats.totalTokens.toLocaleString()} Total Tokens Used`}
        subtitle=""
        options={tokensChartOptions}
        isLoading={isLoading}
      />
    </div>
  );
}

// Re-export types for external use
export type {
  TraceChartsProps,
  TimeBucket,
  DurationBucket,
  TraceStats,
} from "./types";
