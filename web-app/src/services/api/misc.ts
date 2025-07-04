import { apiClient } from "./axios";
import { Artifact } from "@/types/artifact";

export class ChartService {
  static async getChart(file_path: string): Promise<string> {
    const response = await apiClient.get("/charts/" + file_path);
    return response.data;
  }
}

export class ArtifactService {
  static async getArtifact(id: string): Promise<Artifact> {
    const response = await apiClient.get(`/artifacts/${id}`);
    return response.data;
  }
}

export class BuilderService {
  static async checkBuilderAvailability(): Promise<{ available: boolean }> {
    const response = await apiClient.get("/builder-availability");
    return response.data;
  }
}
