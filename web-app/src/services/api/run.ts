import type { PaginationState } from "@tanstack/react-table";
import { apiBaseURL } from "../env";
import type {
  BlockEvent,
  CreateAgenticRunPayload,
  CreateAgenticRunResponse,
  CreateRunPayload,
  CreateRunResponse,
  GetBlocksRequest,
  GetBlocksResponse,
  ListRunsResponse,
  StreamEventsPayload
} from "../types";
import { apiClient } from "./axios";
import fetchSSE from "./fetchSSE";

export class RunService {
  static async streamEvents(
    projectId: string,
    branchName: string,
    payload: StreamEventsPayload,
    onMessage: (event: BlockEvent) => void,
    onClose?: () => void,
    onError?: (error: Error) => void,
    signal?: AbortSignal | null
  ): Promise<void> {
    const searchParams = new URLSearchParams({
      source_id: payload.sourceId,
      run_index: `${payload.runIndex}`,
      branch: branchName
    });
    const url = `${apiBaseURL}/${projectId}/events?${searchParams.toString()}`;
    await fetchSSE(url, {
      method: "GET",
      signal,
      onMessage,
      onClose,
      onError
    });
  }

  static async listRuns(
    projectId: string,
    branchName: string,
    workflowId: string,
    pagination: PaginationState
  ): Promise<ListRunsResponse> {
    const searchParams = new URLSearchParams();
    searchParams.append("branch", branchName);
    if (pagination) {
      searchParams.append("page", `${pagination.pageIndex + 1}`);
      searchParams.append("size", `${pagination.pageSize}`);
    }
    const response = await apiClient.get(
      `/${projectId}/workflows/${btoa(workflowId)}/runs?${searchParams.toString()}`
    );
    return response.data;
  }

  static async getBlocks(
    projectId: string,
    branchName: string,
    payload: GetBlocksRequest
  ): Promise<GetBlocksResponse[]> {
    const searchParams = new URLSearchParams({
      source_id: payload.source_id,
      ...(payload.run_index ? { run_index: `${payload.run_index}` } : {})
    });
    const response = await apiClient.get(`/${projectId}/blocks?${searchParams.toString()}`);
    const data = response.data as GetBlocksResponse;

    const nested = await Promise.allSettled(
      Object.values(data.blocks || {})
        .filter((block) => block.type === "group")
        .map((block) => {
          const [source_id, run_index] = block.group_id.split("::");
          return RunService.getBlocks(projectId, branchName, {
            source_id,
            run_index: run_index ? parseInt(run_index, 10) : undefined
          });
        })
    );
    const flatten = nested.flatMap((result) => {
      if (result.status === "rejected") {
        console.error("Failed to fetch nested blocks:", result.reason);
        return [];
      } else if (result.status === "fulfilled") {
        return result.value;
      }
      return [];
    });
    return [data, ...flatten];
  }

  static async createRun(
    projectId: string,
    branchName: string,
    payload: CreateRunPayload
  ): Promise<CreateRunResponse> {
    const workflowId = btoa(payload.workflowId);
    const response = await apiClient.post(
      `/${projectId}/workflows/${workflowId}/runs`,
      payload.retryType,
      { params: { branch: branchName } }
    );
    return response.data;
  }

  static async cancelRun(
    projectId: string,
    branchName: string,
    sourceId: string,
    runIndex: number
  ): Promise<void> {
    const response = await apiClient.delete(`/${projectId}/runs/${btoa(sourceId)}/${runIndex}`, {
      params: { branch: branchName }
    });
    return response.data;
  }

  static async deleteRun(
    projectId: string,
    branchName: string,
    workflowId: string,
    runIndex: number
  ): Promise<void> {
    await apiClient.delete(`/${projectId}/workflows/${btoa(workflowId)}/runs/${runIndex}`, {
      params: { branch: branchName }
    });
  }

  static async createAgenticRun(
    projectId: string,
    branchName: string,
    payload: CreateAgenticRunPayload
  ): Promise<CreateAgenticRunResponse> {
    const response = await apiClient.post(
      `/${projectId}/threads/${payload.threadId}/agentic`,
      {
        question: payload.prompt,
        agent_ref: payload.agentRef
      },
      { params: { branch: branchName } }
    );
    return response.data;
  }
}
