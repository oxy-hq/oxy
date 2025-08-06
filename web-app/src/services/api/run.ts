import { apiClient } from "./axios";
import fetchSSE from "./fetchSSE";
import { apiBaseURL } from "../env";
import {
  BlockEvent,
  ListRunsResponse,
  CreateRunPayload,
  CreateRunResponse,
  StreamEventsPayload,
  GetBlocksRequest,
  GetBlocksResponse,
} from "../types";
import { PaginationState } from "@tanstack/react-table";

export class RunService {
  static async streamEvents(
    payload: StreamEventsPayload,
    onMessage: (event: BlockEvent) => void,
    onClose?: () => void,
    onError?: (error: Error) => void,
    signal?: AbortSignal | null,
  ): Promise<void> {
    const searchParams = new URLSearchParams({
      source_id: payload.sourceId,
      run_index: `${payload.runIndex}`,
    });
    const url = `${apiBaseURL}/events?${searchParams.toString()}`;
    await fetchSSE(url, {
      method: "GET",
      signal,
      onMessage,
      onClose,
      onError,
    });
  }

  static async listRuns(
    workflowId: string,
    pagination: PaginationState,
  ): Promise<ListRunsResponse> {
    const searchParams = new URLSearchParams();
    if (pagination) {
      searchParams.append("page", `${pagination.pageIndex + 1}`);
      searchParams.append("size", `${pagination.pageSize}`);
    }
    const response = await apiClient.get(
      `/workflows/${btoa(workflowId)}/runs?${searchParams.toString()}`,
    );
    return response.data;
  }

  static async getBlocks(
    payload: GetBlocksRequest,
  ): Promise<GetBlocksResponse[]> {
    const searchParams = new URLSearchParams({
      source_id: payload.source_id,
      ...(payload.run_index ? { run_index: `${payload.run_index}` } : {}),
    });
    const response = await apiClient.get(`/blocks?${searchParams.toString()}`);
    const data = response.data as GetBlocksResponse;

    const nested = await Promise.allSettled(
      Object.values(data.blocks || {})
        .filter((block) => block.type === "group")
        .map((block) => {
          const [source_id, run_index] = block.group_id.split("::");
          return RunService.getBlocks({
            source_id,
            run_index: run_index ? parseInt(run_index, 10) : undefined,
          });
        }),
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
    payload: CreateRunPayload,
  ): Promise<CreateRunResponse> {
    const workflowId = btoa(payload.workflowId);
    const response = await apiClient.post(
      `/workflows/${workflowId}/runs`,
      payload,
    );
    return response.data;
  }

  static async cancelRun(sourceId: string, runIndex: number): Promise<void> {
    const response = await apiClient.delete(
      `/runs/${btoa(sourceId)}/${runIndex}`,
    );
    return response.data;
  }
}
