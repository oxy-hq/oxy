import { apiBaseURL } from "../env";
import type { UiBlock } from "./analytics";
import { apiClient } from "./axios";

export interface CreateAppRunRequest {
  agent_id: string;
  request: string;
  thread_id?: string;
}

export interface CreateAppRunResponse {
  run_id: string;
  thread_id?: string;
}

export interface SaveAppRunResponse {
  app_path64: string;
  app_path: string;
}

export interface AppBuilderRunSummary {
  run_id: string;
  status: "running" | "suspended" | "done" | "failed";
  agent_id: string;
  /** Natural-language request (stored as `question` in DB). */
  request: string;
  /** Generated YAML, present when status is "done". */
  yaml?: string;
  error_message?: string | null;
  ui_events?: UiBlock[];
}

export class AppBuilderService {
  static async createRun(
    projectId: string,
    body: CreateAppRunRequest
  ): Promise<CreateAppRunResponse> {
    const response = await apiClient.post(`/${projectId}/app-builder/app-runs`, body);
    return response.data;
  }

  static async getRunsByThread(
    projectId: string,
    threadId: string
  ): Promise<AppBuilderRunSummary[]> {
    const response = await apiClient.get(`/${projectId}/app-builder/threads/${threadId}/runs`);
    return response.data;
  }

  static async submitAnswer(projectId: string, runId: string, answer: string): Promise<void> {
    await apiClient.post(`/${projectId}/app-builder/app-runs/${runId}/answer`, { answer });
  }

  static async cancelRun(projectId: string, runId: string): Promise<void> {
    await apiClient.post(`/${projectId}/app-builder/app-runs/${runId}/cancel`);
  }

  static async retryRun(projectId: string, runId: string): Promise<{ run_id: string }> {
    const response = await apiClient.post(`/${projectId}/app-builder/app-runs/${runId}/retry`);
    return response.data;
  }

  /** Save the completed run's YAML to disk and return the base64 path for AppPreview. */
  static async saveRun(projectId: string, runId: string): Promise<SaveAppRunResponse> {
    const response = await apiClient.post(`/${projectId}/apps/save-from-run/${runId}`);
    return response.data;
  }

  /** Returns the URL for the SSE event stream. */
  static eventsUrl(projectId: string, runId: string): string {
    return `${apiBaseURL}/${projectId}/app-builder/app-runs/${runId}/events`;
  }
}
