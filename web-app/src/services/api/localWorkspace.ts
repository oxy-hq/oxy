import { apiClient } from "./axios";

export interface SetupEmptyResponse {
  path: string;
  config_created: boolean;
}

export interface SetupDemoResponse {
  path: string;
  files_written: string[];
  files_skipped: string[];
  files_failed: { path: string; error: string }[];
}

export const LocalWorkspaceService = {
  async setupEmpty(workspaceId: string): Promise<SetupEmptyResponse> {
    const response = await apiClient.post(`/${workspaceId}/setup/empty`);
    return response.data;
  },

  async setupDemo(workspaceId: string): Promise<SetupDemoResponse> {
    const response = await apiClient.post(`/${workspaceId}/setup/demo`);
    return response.data;
  }
};
