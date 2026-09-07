import { AxiosError } from "axios";
import type {
  Policy,
  QueuedRuns,
  RunDetail,
  RunListPage,
  SimulationRun,
  SimulationSpecInput,
  SimulationSummary,
  ValidateResponse
} from "@/types/simulation";
import { apiClient } from "./axios";

function rethrow(error: unknown): never {
  if (error instanceof AxiosError && error.response?.data?.message) {
    throw new Error(error.response.data.message);
  }
  throw error;
}

/** Client for the `/simulations*` endpoints. */
export class SimulationService {
  /** The grid of declared worlds this revision carries. */
  static async listSimulations(
    projectId: string,
    branchName?: string
  ): Promise<SimulationSummary[]> {
    try {
      const response = await apiClient.get<SimulationSummary[]>(`/${projectId}/simulations`, {
        params: branchName ? { branch: branchName } : undefined
      });
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }

  /**
   * Queue one or more runs of a world. Returns as soon as they are enqueued — a
   * 40-period run is minutes of warehouse queries, so progress is read back
   * through `getRun`.
   *
   * Plural because the arms of a profit race are runs of the SAME world on the
   * same seed, and because a marginal world declares `replicates:` and fans out
   * onto several draws. One arm and one draw is just the common case.
   *
   * Resolves with `partial_failure` set when only some arms queued — those in
   * `runs` are executing; do not retry them.
   */
  static async startRun(
    projectId: string,
    name: string,
    options: { policies?: Policy[]; replicates?: number; branchName?: string } = {}
  ): Promise<QueuedRuns> {
    try {
      const response = await apiClient.post<QueuedRuns>(
        `/${projectId}/simulations/${encodeURIComponent(name)}/runs`,
        undefined,
        {
          params: {
            ...(options.branchName ? { branch: options.branchName } : {}),
            ...(options.policies?.length ? { policies: options.policies.join(",") } : {}),
            ...(options.replicates ? { replicates: options.replicates } : {})
          }
        }
      );
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }

  /**
   * Checks a candidate world against the same rules `SimulationSpec::from_yaml`
   * runs at run-queue time — an unreachable optimum, an absorbing lever floor,
   * too little history to clear the fitter's floor — before anything is
   * written to a `.simulation.yml`. Never throws on an incoherent spec: `ok:
   * false` with a readable `error` is the expected answer for a form still
   * being filled in, not a request failure.
   */
  static async validateSpec(
    projectId: string,
    spec: SimulationSpecInput
  ): Promise<ValidateResponse> {
    try {
      const response = await apiClient.post<ValidateResponse>(
        `/${projectId}/simulations/validate`,
        spec
      );
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }

  /** This workspace's runs, newest first by enqueue time. */
  static async listRuns(projectId: string, page: RunListPage = {}): Promise<SimulationRun[]> {
    try {
      const response = await apiClient.get<SimulationRun[]>(`/${projectId}/simulations/runs`, {
        params: {
          ...(page.limit !== undefined ? { limit: page.limit } : {}),
          ...(page.offset !== undefined ? { offset: page.offset } : {})
        }
      });
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }

  /**
   * One run with every period and every scored edge.
   *
   * Readable while the run is still going: periods are persisted as they land,
   * so polling this is what makes "watch it happen" work.
   */
  static async getRun(projectId: string, runId: string): Promise<RunDetail> {
    try {
      const response = await apiClient.get<RunDetail>(`/${projectId}/simulations/runs/${runId}`);
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }
}
