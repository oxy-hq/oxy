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

export interface BuilderAvailability {
  available: boolean;
  /** Set for legacy path-based agents; absent for built-in. */
  builder_path?: string;
  /** True when the built-in copilot is configured. */
  builtin?: boolean;
  /** Model name for the built-in copilot. */
  model?: string;
}

export class BuilderService {
  static async checkBuilderAvailability(
    projectId: string
  ): Promise<BuilderAvailability> {
    const response = await apiClient.get(`/${projectId}/builder-availability`);
    return response.data;
  }
}
