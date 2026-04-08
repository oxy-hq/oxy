export interface Workspace {
  id: string;
  name: string;
  workspace_id: string;
  project_repo_id?: string;
  active_branch: WorkspaceBranch | null;
  created_at: string;
  updated_at: string;
}

export interface WorkspacesResponse {
  projects: Workspace[];
  total: number;
}

export interface CreateWorkspaceResponse {
  branch_id: string;
  local_path: string;
  message: string;
  project_id: string;
  success: boolean;
}

export interface WorkspaceBranch {
  name: string;
  sync_status: string;
  revision: string;
  id: string;
  created_at: string;
  updated_at: string;
  branch_type: "local" | "remote";
}

export interface WorkspaceBranchesResponse {
  branches: WorkspaceBranch[];
}
