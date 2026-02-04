import type { Artifact } from "@/types/artifact";
import { apiClient } from "./axios";

export class ChartService {
  static async getChart(projectId: string, branchName: string, file_path: string): Promise<string> {
    const response = await apiClient.get(`/${projectId}/charts/${file_path}`, {
      params: {
        branch: branchName
      }
    });
    return response.data;
  }
}

export class ArtifactService {
  static async getArtifact(projectId: string, branchName: string, id: string): Promise<Artifact> {
    const response = await apiClient.get(`/${projectId}/artifacts/${id}`, {
      params: {
        branch: branchName
      }
    });
    return response.data;
  }
}

export class BuilderService {
  static async checkBuilderAvailability(
    projectId: string
  ): Promise<{ available: boolean; builder_path?: string }> {
    const response = await apiClient.get(`/${projectId}/builder-availability`);
    return response.data;
  }
}
