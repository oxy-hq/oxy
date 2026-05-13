export interface Project {
  id: string;
  name: string;
  workspace_id: string;
  active_branch: ProjectBranch | null;
  created_at: string;
  updated_at: string;
}

export interface ProjectsResponse {
  projects: Project[];
  total: number;
}

export interface CreateProjectResponse {
  branch_id: string;
  local_path: string;
  message: string;
  project_id: string;
  success: boolean;
}

export type BranchOrigin = "local_only" | "remote_only" | "both";

export interface ProjectBranch {
  name: string;
  revision: string;
  id: string;
  created_at: string;
  updated_at: string;
  branch_type: "local" | "remote";
  /** Where this branch lives — drives the WorkspaceBranchSwitcher badges. */
  origin: BranchOrigin;
}

export interface ProjectBranchesResponse {
  branches: ProjectBranch[];
}
