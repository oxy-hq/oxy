import { apiClient } from "./axios";
import { LogsResponse } from "../../types/logs";

export class LogsService {
  static async getLogs(projectId: string): Promise<LogsResponse> {
    const response = await apiClient.get<LogsResponse>(`/${projectId}/logs`);
    return response.data;
  }
}
