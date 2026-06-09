import { AxiosError } from "axios";
import type {
  DistributionRequest,
  ExplainRequest,
  ExplainResult,
  MetricTree,
  OpportunityRequest,
  OpportunityResult,
  PredictChange,
  PredictResult,
  SensitivityResult,
  TimeDimensionsResponse
} from "@/types/metricTree";
import { apiClient } from "./axios";

function rethrow(error: unknown): never {
  if (error instanceof AxiosError && error.response?.data?.message) {
    throw new Error(error.response.data.message);
  }
  throw error;
}

/** Client for the `/semantic/metric-tree*` endpoints. */
export class MetricTreeService {
  /** The full metric tree, or the subtree rooted at `root` when provided. */
  static async getTree(projectId: string, root?: string): Promise<MetricTree> {
    try {
      const response = await apiClient.get<MetricTree>(`/${projectId}/semantic/metric-tree`, {
        params: root ? { root } : {}
      });
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }

  /** Ranked drivers of a measure. */
  static async getSensitivity(projectId: string, measureId: string): Promise<SensitivityResult> {
    try {
      const response = await apiClient.get<SensitivityResult>(
        `/${projectId}/semantic/metric-tree/${encodeURIComponent(measureId)}/sensitivity`
      );
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }

  /** Propagate hypothetical `(measure, delta)` changes upward through the tree. */
  static async predict(projectId: string, changes: PredictChange[]): Promise<PredictResult> {
    try {
      const response = await apiClient.post<PredictResult>(
        `/${projectId}/semantic/metric-tree/predict`,
        { changes }
      );
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }

  /** Period-over-period root-cause decomposition. */
  static async explain(projectId: string, request: ExplainRequest): Promise<ExplainResult> {
    try {
      const response = await apiClient.post<ExplainResult>(
        `/${projectId}/semantic/metric-tree/explain`,
        request
      );
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }

  /** Time dimensions available per view (`view.dim` ids). */
  static async timeDimensions(projectId: string): Promise<TimeDimensionsResponse> {
    try {
      const response = await apiClient.get<TimeDimensionsResponse>(
        `/${projectId}/semantic/metric-tree/time-dimensions`
      );
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }

  /** Single-period distribution; baseline auto-derived server-side. */
  static async distribution(
    projectId: string,
    request: DistributionRequest
  ): Promise<ExplainResult> {
    try {
      const response = await apiClient.post<ExplainResult>(
        `/${projectId}/semantic/metric-tree/distribution`,
        request
      );
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }

  /** Segment opportunity sizing for a measure over a period. */
  static async opportunity(
    projectId: string,
    request: OpportunityRequest
  ): Promise<OpportunityResult> {
    try {
      const response = await apiClient.post<OpportunityResult>(
        `/${projectId}/semantic/metric-tree/opportunity`,
        request
      );
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }
}
