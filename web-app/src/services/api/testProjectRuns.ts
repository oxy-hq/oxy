import { apiClient } from "./axios";

export interface FileScore {
  source_id: string;
  run_index: number;
  score: number | null;
}

export interface TestProjectRunInfo {
  id: string;
  project_id: string;
  name: string | null;
  created_at: string;
  /** Aggregate score (0.0–1.0) across all files, null if no cases */
  score: number | null;
  /** Per-file score breakdown */
  file_scores: FileScore[];
  /** Total number of test cases across all files */
  total_cases: number | null;
  /** Consistency (0.0–1.0): avg(passing_runs / total_runs) per case */
  consistency: number | null;
  /** Sum of avg_duration_ms across all cases */
  total_duration_ms: number | null;
  /** Sum of input + output tokens across all cases */
  total_tokens: number | null;
}

export class TestProjectRunService {
  static async createProjectRun(
    projectId: string,
    name?: string
  ): Promise<TestProjectRunInfo> {
    const response = await apiClient.post(
      `/${projectId}/tests/project-runs`,
      { name: name ?? null }
    );
    return response.data;
  }

  static async listProjectRuns(projectId: string): Promise<TestProjectRunInfo[]> {
    const response = await apiClient.get(`/${projectId}/tests/project-runs`);
    return response.data;
  }

  static async deleteProjectRun(projectId: string, projectRunId: string): Promise<void> {
    await apiClient.delete(`/${projectId}/tests/project-runs/${projectRunId}`);
  }
}
