import { apiClient } from "./axios";
import {
  Workspace,
  WorkspaceListResponse,
  CreateWorkspaceRequest,
} from "@/types/workspace";

export class WorkspaceService {
  static async listWorkspaces(): Promise<WorkspaceListResponse> {
    const response = await apiClient.get("/workspaces");
    return response.data;
  }

  static async createWorkspace(
    request: CreateWorkspaceRequest,
  ): Promise<Workspace> {
    const response = await apiClient.post("/workspaces", request);
    return response.data;
  }
}
