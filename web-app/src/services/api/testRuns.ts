import { apiClient } from "./axios";

export interface TestRunInfo {
  id: string;
  source_id: string;
  run_index: number;
  project_id: string;
  name: string | null;
  created_at: string;
  project_run_id: string | null;
  /** Aggregate score (0.0–1.0), null if no cases recorded yet */
  score: number | null;
}

export interface TestRunCaseResult {
  id: string;
  case_index: number;
  prompt: string;
  expected: string;
  actual_output: string | null;
  score: number;
  verdict: "pass" | "fail" | "flaky";
  passing_runs: number;
  total_runs: number;
  avg_duration_ms: number | null;
  input_tokens: number | null;
  output_tokens: number | null;
  judge_reasoning: string[] | null;
  errors: string[] | null;
  human_verdict: string | null;
}

export interface TestRunWithCases extends TestRunInfo {
  cases: TestRunCaseResult[];
}

export class TestRunService {
  static async createRun(
    projectId: string,
    pathb64: string,
    name?: string,
    projectRunId?: string
  ): Promise<TestRunInfo> {
    const response = await apiClient.post(
      `/${projectId}/tests/${encodeURIComponent(pathb64)}/runs`,
      { name: name ?? null, project_run_id: projectRunId ?? null }
    );
    return response.data;
  }

  static async listRuns(projectId: string, pathb64: string): Promise<TestRunInfo[]> {
    const response = await apiClient.get(`/${projectId}/tests/${encodeURIComponent(pathb64)}/runs`);
    return response.data;
  }

  static async getRun(
    projectId: string,
    pathb64: string,
    runIndex: number
  ): Promise<TestRunWithCases> {
    const response = await apiClient.get(
      `/${projectId}/tests/${encodeURIComponent(pathb64)}/runs/${runIndex}`
    );
    return response.data;
  }

  static async deleteRun(projectId: string, pathb64: string, runIndex: number): Promise<void> {
    await apiClient.delete(`/${projectId}/tests/${encodeURIComponent(pathb64)}/runs/${runIndex}`);
  }

  static async listHumanVerdicts(
    projectId: string,
    pathb64: string,
    runIndex: number
  ): Promise<{ case_index: number; verdict: string; run_index: number }[]> {
    const response = await apiClient.get(
      `/${projectId}/tests/${encodeURIComponent(pathb64)}/runs/${runIndex}/human-verdicts`
    );
    return response.data;
  }

  static async setHumanVerdict(
    projectId: string,
    pathb64: string,
    runIndex: number,
    caseIndex: number,
    verdict: string | null
  ): Promise<{ case_index: number; verdict: string; run_index: number } | null> {
    const response = await apiClient.put(
      `/${projectId}/tests/${encodeURIComponent(pathb64)}/runs/${runIndex}/cases/${caseIndex}/human-verdict`,
      { verdict }
    );
    return response.data;
  }
}
