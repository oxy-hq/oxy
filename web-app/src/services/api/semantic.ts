import { AxiosError } from "axios";
import { apiClient } from "./axios";

type FilterValue = string | number | boolean | null;

interface BaseFilter {
  field: string;
}

export type DateRangeValue = string | Date;

export type SemanticQueryFilter =
  | (BaseFilter & {
      op: "eq" | "neq" | "gt" | "gte" | "lt" | "lte";
      value: FilterValue;
    })
  | (BaseFilter & {
      op: "in" | "not_in";
      values: FilterValue[];
    })
  | (BaseFilter & {
      op: "in_date_range" | "not_in_date_range";
      relative?: string;
      from?: DateRangeValue;
      to?: DateRangeValue;
    });

export interface SemanticQueryOrder {
  field: string;
  direction: "asc" | "desc";
}

type TimeDimension = {
  dimension: string;
  granularity?:
    | "year"
    | "quarter"
    | "month"
    | "week"
    | "day"
    | "hour"
    | "minute"
    | "second"
    | string;
};

interface SemanticQueryParams {
  topic?: string;
  measures?: string[];
  dimensions?: string[];
  timeDimensions?: TimeDimension[];
  filters?: SemanticQueryFilter[];
  orders?: SemanticQueryOrder[];
  limit?: number;
  variables?: Record<string, unknown>;
}

export interface SemanticQueryRequest {
  query: SemanticQueryParams;
  session_filters?: Record<string, unknown>;
  connections?: Record<string, unknown>;
  result_format?: "json" | "parquet";
}

export interface SemanticQueryCompileResponse {
  sql: string;
}

interface Dimension {
  name: string;
  type: string;
  description?: string;
  expr: string;
}

interface Measure {
  name: string;
  type: string;
  description?: string;
  expr?: string;
}

export interface ViewResponse {
  view_name: string;
  name: string;
  description?: string;
  datasource?: string;
  table?: string;
  dimensions: Dimension[];
  measures: Measure[];
}

interface TopicResponse {
  name: string;
  description?: string;
  views: string[];
  base_view?: string;
}

export interface TopicDetailsResponse {
  topic: TopicResponse;
  views: ViewResponse[];
}

export type ExecuteSemanticQueryResponse =
  | string[][] // JSON format - returns array directly
  | { file_name: string; is_preagg: boolean; execution_time_ms: number }; // Parquet format

export class SemanticService {
  static async executeSemanticQuery(
    projectId: string,
    request: SemanticQueryRequest
  ): Promise<ExecuteSemanticQueryResponse> {
    const { query, ...rest } = request;
    try {
      const response = await apiClient.post(`/${projectId}/semantic`, {
        ...query,
        ...rest
      });
      return response.data;
    } catch (error) {
      if (error instanceof AxiosError && error.response?.data?.message) {
        throw new Error(error.response.data.message);
      }
      throw error;
    }
  }

  static async compileSemanticQuery(
    projectId: string,
    request: SemanticQueryRequest
  ): Promise<SemanticQueryCompileResponse> {
    const { query, ...rest } = request;
    try {
      const response = await apiClient.post(`/${projectId}/semantic/compile`, {
        ...query,
        ...rest
      });
      return response.data;
    } catch (error) {
      if (error instanceof AxiosError && error.response?.data?.message) {
        throw new Error(error.response.data.message);
      }
      throw error;
    }
  }

  static async getTopicDetails(
    projectId: string,
    filePathB64: string,
    branchName?: string
  ): Promise<TopicDetailsResponse> {
    try {
      const params = branchName ? { branch: branchName } : {};
      const response = await apiClient.get(`/${projectId}/semantic/topic/${filePathB64}`, {
        params
      });
      return response.data;
    } catch (error) {
      if (error instanceof AxiosError && error.response?.data?.message) {
        throw new Error(error.response.data.message);
      }
      throw error;
    }
  }

  static async getViewDetails(
    projectId: string,
    filePathB64: string,
    branchName?: string
  ): Promise<ViewResponse> {
    try {
      const params = branchName ? { branch: branchName } : {};
      const response = await apiClient.get(`/${projectId}/semantic/view/${filePathB64}`, {
        params
      });
      return response.data;
    } catch (error) {
      if (error instanceof AxiosError && error.response?.data?.message) {
        throw new Error(error.response.data.message);
      }
      throw error;
    }
  }
}

// ── Preagg status ─────────────────────────────────────────────────────────────

type PreaggMeasure = {
  name: string;
  measure_type: string;
};

export type PreaggRollupStatus = {
  view_name: string;
  rollup_name: string;
  has_parquet: boolean;
  dimensions: string[];
  measures: PreaggMeasure[];
  time_dimension: string | null;
  granularity: string | null;
  build_date: string | null;
  refresh_key_checked_at: string | null;
};

export type PreaggStatusResponse = {
  rollups: PreaggRollupStatus[];
};

export async function getPreaggStatus(
  workspaceId: string,
  branchName?: string
): Promise<PreaggStatusResponse> {
  const params = branchName ? { branch: branchName } : {};
  const { data } = await apiClient.get<PreaggStatusResponse>(
    `/${workspaceId}/semantic/preagg-status`,
    { params }
  );
  return data;
}
