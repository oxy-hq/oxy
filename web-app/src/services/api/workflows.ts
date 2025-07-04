import { apiClient } from "./axios";
import { Workflow } from "@/types/workflow";
import { LogItem } from "../types";
import fetchSSE from "./fetchSSE";
import { apiBaseURL } from "../env";

export class WorkflowService {
  static async createWorkflowFromQuery(request: {
    query: string;
    prompt: string;
    database: string;
  }): Promise<{ workflow: Workflow }> {
    const response = await apiClient.post("/workflows/from-query", request);
    return response.data;
  }

  static async runWorkflow(
    pathb64: string,
    onLogItem: (logItem: LogItem) => void,
  ): Promise<void> {
    const url = `${apiBaseURL}/workflows/${pathb64}/run`;
    await fetchSSE(url, {
      onMessage: onLogItem,
    });
  }

  static async runWorkflowThread(
    threadId: string,
    onLogItem: (logItem: LogItem) => void,
  ): Promise<void> {
    const url = `${apiBaseURL}/threads/${threadId}/workflow`;
    await fetchSSE(url, {
      onMessage: onLogItem,
    });
  }
}
