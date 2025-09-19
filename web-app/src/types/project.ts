export interface Project {
  id: string;
  name: string;
  organization_id: string;
  repo_id: number;
  active_branch: ProjectBranch | null;
  created_at: string;
  updated_at: string;
}

export interface ProjectsResponse {
  projects: Project[];
  total: number;
}

export interface CreateProjectRequest {
  repo_id: number;
  token: string;
  branch: string;
  provider: string;
}

export interface CreateProjectResponse {
  branch_id: string;
  local_path: string;
  message: string;
  project_id: string;
  success: boolean;
}

export interface ProjectBranch {
  name: string;
  sync_status: string;
  revision: string;
  id: string;
  created_at: string;
  updated_at: string;
  branch_type: "local" | "remote";
}

export interface ProjectBranchesResponse {
  branches: ProjectBranch[];
}
