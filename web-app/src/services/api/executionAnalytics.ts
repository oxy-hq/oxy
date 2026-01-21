import { apiClient } from "./axios";
import {
  ExecutionSummary,
  ExecutionTimeBucket,
  AgentExecutionStats,
  ExecutionDetail,
} from "@/pages/ide/observability/execution-analytics/types";

export interface ExecutionListResponse {
  executions: ExecutionDetail[];
  total: number;
  limit: number;
  offset: number;
}

export interface SummaryQuery {
  days?: number;
}

export interface TimeSeriesQuery {
  days?: number;
}

export interface AgentStatsQuery {
  days?: number;
  limit?: number;
}

export interface ExecutionsQuery {
  days?: number;
  limit?: number;
  offset?: number;
  executionType?: string;
  isVerified?: boolean;
  sourceRef?: string;
  status?: string;
}

export class ExecutionAnalyticsService {
  static async getSummary(
    projectId: string,
    params?: SummaryQuery,
  ): Promise<ExecutionSummary> {
    const urlParams = new URLSearchParams();
    if (params?.days !== undefined)
      urlParams.append("days", params.days.toString());

    let url = `/${projectId}/execution-analytics/summary`;
    const paramsStr = urlParams.toString();
    if (paramsStr) {
      url += "?" + paramsStr;
    }
    const response = await apiClient.get(url);
    return response.data;
  }

  static async getTimeSeries(
    projectId: string,
    params?: TimeSeriesQuery,
  ): Promise<ExecutionTimeBucket[]> {
    const urlParams = new URLSearchParams();
    if (params?.days !== undefined)
      urlParams.append("days", params.days.toString());

    let url = `/${projectId}/execution-analytics/time-series`;
    const paramsStr = urlParams.toString();
    if (paramsStr) {
      url += "?" + paramsStr;
    }
    const response = await apiClient.get(url);
    return response.data;
  }

  static async getAgentStats(
    projectId: string,
    params?: AgentStatsQuery,
  ): Promise<AgentExecutionStats[]> {
    const urlParams = new URLSearchParams();
    if (params?.days !== undefined)
      urlParams.append("days", params.days.toString());
    if (params?.limit !== undefined)
      urlParams.append("limit", params.limit.toString());

    let url = `/${projectId}/execution-analytics/agents`;
    const paramsStr = urlParams.toString();
    if (paramsStr) {
      url += "?" + paramsStr;
    }
    const response = await apiClient.get(url);
    return response.data;
  }

  static async getExecutions(
    projectId: string,
    params?: ExecutionsQuery,
  ): Promise<ExecutionListResponse> {
    const urlParams = new URLSearchParams();
    if (params?.days !== undefined)
      urlParams.append("days", params.days.toString());
    if (params?.limit !== undefined)
      urlParams.append("limit", params.limit.toString());
    if (params?.offset !== undefined)
      urlParams.append("offset", params.offset.toString());
    if (params?.executionType)
      urlParams.append("execution_type", params.executionType);
    if (params?.isVerified !== undefined)
      urlParams.append("is_verified", params.isVerified.toString());
    if (params?.sourceRef) urlParams.append("source_ref", params.sourceRef);
    if (params?.status) urlParams.append("status", params.status);

    let url = `/${projectId}/execution-analytics/executions`;
    const paramsStr = urlParams.toString();
    if (paramsStr) {
      url += "?" + paramsStr;
    }
    const response = await apiClient.get(url);
    return response.data;
  }
}

// Convert time range to days
export function timeRangeToDays(timeRange: string): number {
  switch (timeRange) {
    case "1h":
      return 1; // API will handle hour-level filtering if needed
    case "24h":
      return 1;
    case "7d":
      return 7;
    case "30d":
      return 30;
    default:
      return 7;
  }
}
