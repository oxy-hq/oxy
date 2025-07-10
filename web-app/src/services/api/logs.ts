import { apiClient } from "./axios";
import { LogsResponse } from "../../types/logs";

export class LogsService {
  static async getLogs(): Promise<LogsResponse> {
    const response = await apiClient.get<LogsResponse>("/logs");
    return response.data;
  }
}
