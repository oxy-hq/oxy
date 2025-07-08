import { ProjectStatus } from "@/types/github";
import { apiClient } from "./api/axios";

export class ProjectService {
  /**
   * Get current project status
   */
  static async getProjectStatus(): Promise<ProjectStatus> {
    const response = await apiClient.get<ProjectStatus>("/project/status");
    return response.data;
  }
}
