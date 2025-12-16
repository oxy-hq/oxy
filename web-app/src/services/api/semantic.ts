import { apiClient } from "./axios";

export interface SemanticQueryFilter {
  field: string;
  op: string;
  value: string | number | boolean | null;
}

export interface SemanticQueryOrder {
  field: string;
  direction: "asc" | "desc";
}

export interface SemanticQueryParams {
  topic?: string;
  measures?: string[];
  dimensions?: string[];
  filters?: SemanticQueryFilter[];
  orders?: SemanticQueryOrder[];
  limit?: number;
  variables?: Record<string, unknown>;
}

export interface SemanticQueryRequest {
  query: SemanticQueryParams;
  session_filters?: Record<string, unknown>;
  connections?: Record<string, unknown>;
}

export interface SemanticQueryCompileResponse {
  sql: string;
}

export interface Dimension {
  name: string;
  type: string;
  description?: string;
  expr: string;
}

export interface Measure {
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

export interface TopicResponse {
  name: string;
  description?: string;
  views: string[];
  base_view?: string;
}

export interface TopicDetailsResponse {
  topic: TopicResponse;
  views: ViewResponse[];
}

export class SemanticService {
  static async executeSemanticQuery(
    projectId: string,
    request: SemanticQueryRequest,
  ): Promise<string[][]> {
    const { query, ...rest } = request;
    try {
      const response = await apiClient.post(`/${projectId}/semantic`, {
        ...query,
        ...rest,
      });
      return response.data;
    } catch (error) {
      const e = error as { response?: { data?: { message?: string } } };
      if (e.response?.data?.message) {
        throw new Error(e.response.data.message);
      }
      throw error;
    }
  }

  static async compileSemanticQuery(
    projectId: string,
    request: SemanticQueryRequest,
  ): Promise<SemanticQueryCompileResponse> {
    const { query, ...rest } = request;
    try {
      const response = await apiClient.post(`/${projectId}/semantic/compile`, {
        ...query,
        ...rest,
      });
      return response.data;
    } catch (error) {
      const e = error as { response?: { data?: { message?: string } } };
      if (e.response?.data?.message) {
        throw new Error(e.response.data.message);
      }
      throw error;
    }
  }

  static async getTopicDetails(
    projectId: string,
    topicName: string,
  ): Promise<TopicDetailsResponse> {
    const response = await apiClient.get(
      `/${projectId}/semantic/topic/${topicName}`,
    );
    return response.data;
  }

  static async getViewDetails(
    projectId: string,
    viewName: string,
  ): Promise<ViewResponse> {
    const response = await apiClient.get(
      `/${projectId}/semantic/view/${viewName}`,
    );
    return response.data;
  }
}
