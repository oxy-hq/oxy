import type { TestStreamMessage } from "@/types/eval";
import { apiBaseURL } from "../env";
import { apiClient } from "./axios";
import fetchSSE from "./fetchSSE";

export interface TestFileSummary {
  path: string;
  name: string | null;
  target: string | null;
  case_count: number;
}

export interface TestFileConfig {
  name: string | null;
  target: string | null;
  settings: {
    concurrency: number;
    runs: number;
    judge_model: string | null;
  };
  cases: Array<{
    prompt: string;
    expected: string;
    tags: string[];
    tool: string | null;
  }>;
}

export class TestFileService {
  static async listTestFiles(projectId: string, branchName: string): Promise<TestFileSummary[]> {
    const response = await apiClient.get(`/${projectId}/tests`, {
      params: { branch: branchName }
    });
    return response.data;
  }

  static async getTestFile(
    projectId: string,
    branchName: string,
    pathb64: string
  ): Promise<TestFileConfig> {
    const response = await apiClient.get(`/${projectId}/tests/${encodeURIComponent(pathb64)}`, {
      params: { branch: branchName }
    });
    return response.data;
  }

  static async runTestCase(
    projectId: string,
    branchName: string,
    pathb64: string,
    caseIndex: number,
    onReadStream: (event: TestStreamMessage) => void,
    runIndex?: number,
    signal?: AbortSignal
  ): Promise<void> {
    const searchParams = new URLSearchParams({
      branch: branchName
    });
    if (runIndex !== undefined) {
      searchParams.set("run_index", String(runIndex));
    }
    const url = `${apiBaseURL}/${projectId}/tests/${encodeURIComponent(pathb64)}/cases/${caseIndex}?${searchParams.toString()}`;
    await fetchSSE(url, {
      onMessage: onReadStream,
      signal
    });
  }
}
