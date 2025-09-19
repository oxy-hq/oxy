import { apiClient } from "./axios";
import { Workflow } from "@/types/workflow";
import { LogItem } from "../types";
import fetchSSE from "./fetchSSE";
import { apiBaseURL } from "../env";
import { WorkflowConfig } from "@/stores/useWorkflow";

export class WorkflowService {
  static async listWorkflows(
    projectId: string,
    branchName: string,
  ): Promise<Workflow[]> {
    const response = await apiClient.get(`/${projectId}/workflows`, {
      params: { branch: branchName },
    });
    return response.data;
  }

  static async getWorkflow(
    projectId: string,
    branchName: string,
    pathb64: string,
  ): Promise<WorkflowConfig> {
    const response = await apiClient.get(`/${projectId}/workflows/${pathb64}`, {
      params: { branch: branchName },
    });
    return response.data.data;
  }

  static async getWorkflowLogs(
    projectId: string,
    branchName: string,
    pathb64: string,
  ): Promise<LogItem[]> {
    const response = await apiClient.get(
      `/${projectId}/workflows/${pathb64}/logs`,
      {
        params: {
          branch: branchName,
        },
      },
    );
    return response.data;
  }

  static async createWorkflowFromQuery(
    projectId: string,
    branchName: string,
    request: {
      query: string;
      prompt: string;
      database: string;
    },
  ): Promise<{ workflow: Workflow }> {
    const response = await apiClient.post(
      `/${projectId}/workflows/from-query`,
      request,
      {
        params: { branch: branchName },
      },
    );
    return response.data;
  }

  static async runWorkflow(
    projectId: string,
    branchName: string,
    pathb64: string,
    onLogItem: (logItem: LogItem) => void,
  ): Promise<void> {
    const searchParams = new URLSearchParams({
      branch: branchName,
    });
    const url = `${apiBaseURL}/${projectId}/workflows/${pathb64}/run?${searchParams.toString()}`;
    await fetchSSE(url, {
      onMessage: onLogItem,
    });
  }

  static async runWorkflowThread(
    projectId: string,
    branchName: string,
    threadId: string,
    onLogItem: (logItem: LogItem) => void,
  ): Promise<void> {
    const searchParams = new URLSearchParams({
      branch: branchName,
    });
    const url = `${apiBaseURL}/${projectId}/threads/${threadId}/workflow?${searchParams.toString()}`;
    await fetchSSE(url, {
      onMessage: onLogItem,
    });
  }
}
