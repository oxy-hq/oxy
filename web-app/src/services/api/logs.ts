import type { LogsResponse } from "../../types/logs";
import { apiClient } from "./axios";

export class LogsService {
  static async getLogs(projectId: string): Promise<LogsResponse> {
    const response = await apiClient.get<LogsResponse>(`/${projectId}/logs`);
    return response.data;
  }
}
