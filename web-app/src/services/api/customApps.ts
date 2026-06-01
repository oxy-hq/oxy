import type { CustomAppSummary } from "@/types/apps";
import { apiClient } from "./axios";

/**
 * Workspace-scoped "Custom Apps" (bespoke JS apps Oxy engineers
 * publish on top of this workspace's data). Powers the workspace
 * sidebar's Custom Apps section.
 */
export const CustomAppsService = {
  async list(workspaceId: string): Promise<CustomAppSummary[]> {
    const response = await apiClient.get(`/${workspaceId}/custom-apps`);
    return response.data;
  }
};
