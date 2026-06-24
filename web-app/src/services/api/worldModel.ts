import type {
  WmFilterCountEvent,
  WmInstanceDetailEvent,
  WmInstancesResponse,
  WmMeasureBreakdownEvent,
  WorldModel
} from "@/types/worldModel";
import { apiBaseURL } from "../env";
import { apiClient } from "./axios";
import fetchSSE from "./fetchSSE";

export class WorldModelService {
  static async getWorldModel(projectId: string): Promise<WorldModel> {
    const response = await apiClient.get<WorldModel>(`/${projectId}/semantic/world-model`);
    return response.data;
  }

  static async getInstances(
    projectId: string,
    entityId: string,
    search?: string,
    limit = 50
  ): Promise<WmInstancesResponse> {
    const response = await apiClient.get<WmInstancesResponse>(
      `/${projectId}/semantic/world-model/instances`,
      { params: { entity: entityId, search: search || undefined, limit } }
    );
    return response.data;
  }

  static streamFilterCounts(
    projectId: string,
    entityId: string,
    keyValue: string,
    onEvent: (event: WmFilterCountEvent) => void,
    onClose: () => void,
    signal: AbortSignal
  ): void {
    fetchSSE<WmFilterCountEvent>(`${apiBaseURL}/${projectId}/semantic/world-model/filter-counts`, {
      method: "POST",
      body: { entity_id: entityId, key_value: keyValue },
      onMessage: onEvent,
      onClose,
      onError: onClose,
      signal
    });
  }

  static streamInstanceDetail(
    projectId: string,
    entityId: string,
    keyValue: string,
    onEvent: (event: WmInstanceDetailEvent) => void,
    onClose: () => void,
    signal: AbortSignal
  ): void {
    const params = new URLSearchParams({ entity: entityId, key: keyValue });
    fetchSSE<WmInstanceDetailEvent>(
      `${apiBaseURL}/${projectId}/semantic/world-model/instance-detail?${params}`,
      { method: "GET", onMessage: onEvent, onClose, onError: onClose, signal }
    );
  }

  static streamMeasureBreakdown(
    projectId: string,
    entityId: string,
    keyValue: string,
    measure: string,
    onEvent: (event: WmMeasureBreakdownEvent) => void,
    onClose: () => void,
    signal: AbortSignal
  ): void {
    const params = new URLSearchParams({ entity: entityId, key: keyValue, measure });
    fetchSSE<WmMeasureBreakdownEvent>(
      `${apiBaseURL}/${projectId}/semantic/world-model/measure-breakdown?${params}`,
      { method: "GET", onMessage: onEvent, onClose, onError: onClose, signal }
    );
  }
}
