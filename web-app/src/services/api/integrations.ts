import { apiClient } from "./axios";

export interface LookerExplore {
  model: string;
  name: string;
  description: string | null;
  fields: string[];
}

export interface LookerIntegrationInfo {
  name: string;
  explores: LookerExplore[];
}

export class IntegrationService {
  static async listLookerIntegrations(
    projectId: string,
    branchName: string
  ): Promise<LookerIntegrationInfo[]> {
    const response = await apiClient.get(`/${projectId}/integrations/looker`, {
      params: { branch: branchName }
    });
    return response.data;
  }
}
