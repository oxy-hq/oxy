import { useQuery } from "@tanstack/react-query";
import type {
  AgentExecutionStats,
  ExecutionDetail,
  ExecutionSummary,
  ExecutionTimeBucket
} from "@/pages/ide/observability/execution-analytics/types";
import {
  type AgentStatsQuery,
  ExecutionAnalyticsService,
  type ExecutionsQuery,
  type SummaryQuery,
  type TimeSeriesQuery
} from "@/services/api/executionAnalytics";

// Query keys for execution analytics
export const executionAnalyticsKeys = {
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
