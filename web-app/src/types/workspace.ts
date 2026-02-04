import type { CreateWorkspaceState } from "@/pages/create-workspace";

export interface ProjectInfo {
  id: string;
  name: string;
  workspace_id: string;
  created_at: string;
  updated_at: string;
}

export interface Workspace {
  id: string;
  name: string;
  role: string;
  created_at: string;
  updated_at: string;
  project?: ProjectInfo;
}

export interface WorkspaceListResponse {
  workspaces: Workspace[];
  total: number;
}

export type CreateWorkspaceRequest = CreateWorkspaceState;

export interface MessageResponse {
  message: string;
}
