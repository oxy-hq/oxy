import { useQuery } from "@tanstack/react-query";
import type {
  AgentExecutionStats,
  CostResponse,
  ExecutionDetail,
  ExecutionSummary,
  ExecutionTimeBucket,
  HistogramResponse,
  PercentilesResponse
} from "@/pages/ide/observability/execution-analytics/types";
import {
  type AgentStatsQuery,
  ExecutionAnalyticsService,
  type ExecutionsQuery,
  type SummaryQuery,
  type TimeSeriesQuery
} from "@/services/api/executionAnalytics";
import queryKeys from "./queryKey";

const executionAnalyticsKeys = {
  all: ["executionAnalytics"] as const,
  summary: (projectId: string, params?: SummaryQuery) =>
    [...executionAnalyticsKeys.all, "summary", projectId, params] as const,
  timeSeries: (projectId: string, params?: TimeSeriesQuery) =>
    [...executionAnalyticsKeys.all, "timeSeries", projectId, params] as const,
  agentStats: (projectId: string, params?: AgentStatsQuery) =>
    [...executionAnalyticsKeys.all, "agentStats", projectId, params] as const,
  executions: (projectId: string, params?: ExecutionsQuery) =>
    [...executionAnalyticsKeys.all, "executions", projectId, params] as const
};

export const useExecutionSummary = (
  projectId: string | undefined,
  params?: SummaryQuery,
  enabled = true
) =>
  useQuery<ExecutionSummary, Error>({
    queryKey: executionAnalyticsKeys.summary(projectId!, params),
    queryFn: () => ExecutionAnalyticsService.getSummary(projectId!, params),
    enabled: enabled && !!projectId
  });

export const useExecutionTimeSeries = (
  projectId: string | undefined,
  params?: TimeSeriesQuery,
  enabled = true
) =>
  useQuery<ExecutionTimeBucket[], Error>({
    queryKey: executionAnalyticsKeys.timeSeries(projectId!, params),
    queryFn: () => ExecutionAnalyticsService.getTimeSeries(projectId!, params),
    enabled: enabled && !!projectId
  });

export const useExecutionAgentStats = (
  projectId: string | undefined,
  params?: AgentStatsQuery,
  enabled = true
) =>
  useQuery<AgentExecutionStats[], Error>({
    queryKey: executionAnalyticsKeys.agentStats(projectId!, params),
    queryFn: () => ExecutionAnalyticsService.getAgentStats(projectId!, params),
    enabled: enabled && !!projectId
  });

export const useExecutions = (
  projectId: string | undefined,
  params?: ExecutionsQuery,
  enabled = true
) =>
  useQuery<{ executions: ExecutionDetail[]; total: number }, Error>({
    queryKey: executionAnalyticsKeys.executions(projectId!, params),
    queryFn: () => ExecutionAnalyticsService.getExecutions(projectId!, params),
    enabled: enabled && !!projectId
  });

// New "technical" analytics endpoints — keyed via the canonical queryKey.ts.
export const useExecutionPercentiles = (
  projectId: string | undefined,
  days?: number,
  enabled = true
) =>
  useQuery<PercentilesResponse, Error>({
    queryKey: queryKeys.executionAnalytics.percentiles(projectId!, days),
    queryFn: () => ExecutionAnalyticsService.getPercentiles(projectId!, { days }),
    enabled: enabled && !!projectId
  });

export const useExecutionHistogram = (
  projectId: string | undefined,
  days?: number,
  enabled = true
) =>
  useQuery<HistogramResponse, Error>({
    queryKey: queryKeys.executionAnalytics.histogram(projectId!, days),
    queryFn: () => ExecutionAnalyticsService.getHistogram(projectId!, { days }),
    enabled: enabled && !!projectId
  });

export const useExecutionCost = (projectId: string | undefined, days?: number, enabled = true) =>
  useQuery<CostResponse, Error>({
    queryKey: queryKeys.executionAnalytics.cost(projectId!, days),
    queryFn: () => ExecutionAnalyticsService.getCost(projectId!, { days }),
    enabled: enabled && !!projectId
  });
