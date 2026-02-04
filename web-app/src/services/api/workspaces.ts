import type { CreateWorkspaceRequest, Workspace, WorkspaceListResponse } from "@/types/workspace";
import { apiClient } from "./axios";

export class WorkspaceService {
  static async listWorkspaces(): Promise<WorkspaceListResponse> {
    const response = await apiClient.get("/workspaces");
    return response.data;
  }

  static async createWorkspace(request: CreateWorkspaceRequest): Promise<Workspace> {
    const payload = {
      ...request,
      ...(request.github
        ? {
            github: {
              ...request.github,
              namespace_id: request.github.namespace?.id || null,
              repo_id: request.github.repository?.id || null
            }
          }
        : undefined)
    };
    const response = await apiClient.post("/workspaces", payload);
    return response.data;
  }
}
