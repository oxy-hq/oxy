import { apiClient } from "./axios";

export interface LookerExplore {
  model: string;
  name: string;
  description: string | null;
  dimensions: string[];
  measures: string[];
}

export interface LookerIntegrationInfo {
  name: string;
  explores: LookerExplore[];
}

export interface LookerQueryRequest {
  integration: string;
  model: string;
  explore: string;
  fields: string[];
  filters?: Record<string, string>;
  sorts?: Array<{ field: string; direction: string }>;
  limit?: number;
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

  static async executeLookerQuery(
    projectId: string,
    branchName: string,
    request: LookerQueryRequest
  ): Promise<{ file_name: string }> {
    const response = await apiClient.post(`/${projectId}/integrations/looker/query`, request, {
      params: { branch: branchName }
    });
    return response.data;
  }

  static async compileLookerQuery(
    projectId: string,
    branchName: string,
    request: LookerQueryRequest
  ): Promise<string> {
    const response = await apiClient.post(`/${projectId}/integrations/looker/query/sql`, request, {
      params: { branch: branchName }
    });
    return response.data;
  }
}
