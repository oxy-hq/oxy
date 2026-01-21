import { apiClient } from "./axios";

// Types matching the Rust backend types

export interface MetricAnalytics {
  name: string;
  count: number;
  last_used: string | null;
  trend: string | null;
}

export interface SourceTypeBreakdown {
  agent: number;
  workflow: number;
  task: number;
}

export interface ContextTypeBreakdown {
  sql: number;
  semantic_query: number;
  question: number;
  response: number;
}

export interface MetricAnalyticsResponse {
  total_queries: number;
  unique_metrics: number;
  avg_per_metric: number;
  most_popular: string | null;
  most_popular_count: number | null;
  trend_vs_last_period: string | null;
  by_source_type: SourceTypeBreakdown;
  by_context_type: ContextTypeBreakdown;
}

export interface MetricsListResponse {
  metrics: MetricAnalytics[];
  total: number;
  limit: number;
  offset: number;
}

export interface UsageTrendPoint {
  date: string;
  count: number;
}

export interface RelatedMetric {
  name: string;
  co_occurrence_count: number;
}

export interface RecentUsage {
  source_type: string;
  source_ref: string;
  context_types: string[];
  context: string | null;
  trace_id: string;
  created_at: string;
}

// Parsed context item from the context JSON
export interface ContextItem {
  type: string;
  content: string | SemanticContent | SemanticContent[];
}

export interface SemanticContent {
  topic: string | null;
  measures: string[];
  dimensions: string[];
}

export interface MetricDetailResponse {
  name: string;
  total_queries: number;
  trend_vs_last_period: string | null;
  via_agent: number;
  via_workflow: number;
  usage_trend: UsageTrendPoint[];
  related_metrics: RelatedMetric[];
  recent_usage: RecentUsage[];
}

export class MetricsService {
  static async getAnalytics(
    projectId: string,
    days: number = 30,
  ): Promise<MetricAnalyticsResponse> {
    const params = new URLSearchParams();
    params.append("days", days.toString());

    const response = await apiClient.get(
      `/${projectId}/metrics/analytics?${params.toString()}`,
    );
    return response.data;
  }

  static async getMetricsList(
    projectId: string,
    days: number = 30,
    limit: number = 20,
    offset: number = 0,
  ): Promise<MetricsListResponse> {
    const params = new URLSearchParams();
    params.append("days", days.toString());
    params.append("limit", limit.toString());
    params.append("offset", offset.toString());

    const response = await apiClient.get(
      `/${projectId}/metrics/list?${params.toString()}`,
    );
    return response.data;
  }

  static async getMetricDetail(
    projectId: string,
    metricName: string,
    days: number = 30,
  ): Promise<MetricDetailResponse> {
    const params = new URLSearchParams();
    params.append("days", days.toString());

    const response = await apiClient.get(
      `/${projectId}/metrics/${encodeURIComponent(metricName)}?${params.toString()}`,
    );
    return response.data;
  }
}
