import { AxiosError } from "axios";
import type {
  BaselineRequest,
  BaselineResponse,
  DistributionRequest,
  DrillRequest,
  DrillResponse,
  ExplainRequest,
  ExplainResult,
  MetricTree,
  OpportunityRequest,
  OpportunityResult,
  PredictChange,
  PredictOptions,
  PredictResult,
  ProjectionRequest,
  ProjectionResponse,
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
  static async getTree(projectId: string, root?: string, branchName?: string): Promise<MetricTree> {
    try {
      const response = await apiClient.get<MetricTree>(`/${projectId}/semantic/metric-tree`, {
        params: {
          ...(root ? { root } : {}),
          ...(branchName ? { branch: branchName } : {})
        }
      });
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }

  /** Ranked drivers of a measure. */
  static async getSensitivity(
    projectId: string,
    measureId: string,
    branchName?: string
  ): Promise<SensitivityResult> {
    try {
      const response = await apiClient.get<SensitivityResult>(
        `/${projectId}/semantic/metric-tree/${encodeURIComponent(measureId)}/sensitivity`,
        { params: branchName ? { branch: branchName } : {} }
      );
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }

  /** Propagate hypothetical `(measure, delta)` changes upward through the tree.
   *  Passing `values` sizes multiplicative edges that would otherwise come
   *  back `unquantifiable`. */
  static async predict(
    projectId: string,
    changes: PredictChange[],
    branchName?: string,
    // One object rather than two more positional optionals: they are the same
    // pair the SDK's `predict` takes, and `PredictOptions` is where their
    // meaning is documented — a caller passing them positionally had to read
    // this signature to learn that omitting `coefficients` silently drops every
    // undeclared edge from the result.
    options: PredictOptions = {}
  ): Promise<PredictResult> {
    const { values, coefficients } = options;
    try {
      const response = await apiClient.post<PredictResult>(
        `/${projectId}/semantic/metric-tree/predict`,
        {
          changes,
          ...(values ? { values } : {}),
          // Sent verbatim, refusals included — the server ignores entries
          // carrying no coefficient, and filtering them here would just be a
          // second place for the two sides to disagree.
          ...(coefficients?.length ? { coefficients } : {})
        },
        { params: branchName ? { branch: branchName } : {} }
      );
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }

  /** Current values for the levers plus everything downstream of them. */
  static async baseline(
    projectId: string,
    request: BaselineRequest,
    branchName?: string
  ): Promise<BaselineResponse> {
    try {
      const response = await apiClient.post<BaselineResponse>(
        `/${projectId}/semantic/metric-tree/baseline`,
        request,
        { params: branchName ? { branch: branchName } : {} }
      );
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }

  /** Bucketed history plus a forward forecast for the levers and everything
   *  downstream of them. Returns the BASELINE curve only — the scenario curve
   *  is composed client-side from this and the `predict` result, so editing a
   *  lever costs no warehouse query. */
  static async projection(
    projectId: string,
    request: ProjectionRequest,
    branchName?: string
  ): Promise<ProjectionResponse> {
    try {
      const response = await apiClient.post<ProjectionResponse>(
        `/${projectId}/semantic/metric-tree/projection`,
        request,
        { params: branchName ? { branch: branchName } : {} }
      );
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }

  /** Period-over-period root-cause decomposition. */
  static async explain(
    projectId: string,
    request: ExplainRequest,
    branchName?: string
  ): Promise<ExplainResult> {
    try {
      const response = await apiClient.post<ExplainResult>(
        `/${projectId}/semantic/metric-tree/explain`,
        request,
        { params: branchName ? { branch: branchName } : {} }
      );
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }

  /** Time dimensions available per view (`view.dim` ids). */
  static async timeDimensions(
    projectId: string,
    branchName?: string
  ): Promise<TimeDimensionsResponse> {
    try {
      const response = await apiClient.get<TimeDimensionsResponse>(
        `/${projectId}/semantic/metric-tree/time-dimensions`,
        { params: branchName ? { branch: branchName } : {} }
      );
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }

  /** Single-period distribution; baseline auto-derived server-side. */
  static async distribution(
    projectId: string,
    request: DistributionRequest,
    branchName?: string
  ): Promise<ExplainResult> {
    try {
      const response = await apiClient.post<ExplainResult>(
        `/${projectId}/semantic/metric-tree/distribution`,
        request,
        { params: branchName ? { branch: branchName } : {} }
      );
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }

  /** Segment opportunity sizing for a measure over a period. */
  static async opportunity(
    projectId: string,
    request: OpportunityRequest,
    branchName?: string
  ): Promise<OpportunityResult> {
    try {
      const response = await apiClient.post<OpportunityResult>(
        `/${projectId}/semantic/metric-tree/opportunity`,
        request,
        { params: branchName ? { branch: branchName } : {} }
      );
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }

  /** Recursive opportunity decomposition for a measure over a period. */
  static async drill(projectId: string, request: DrillRequest): Promise<DrillResponse> {
    try {
      const response = await apiClient.post<DrillResponse>(
        `/${projectId}/semantic/metric-tree/drill`,
        request
      );
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }
}
