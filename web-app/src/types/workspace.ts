export interface ProjectInfo {
  id: string;
  name: string;
  workspace_id: string;
  provider?: string;
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

export interface CreateWorkspaceRequest {
  name: string;
  repo_id?: number;
  token?: string;
  branch?: string;
  provider?: string;
}

export interface MessageResponse {
  message: string;
}
