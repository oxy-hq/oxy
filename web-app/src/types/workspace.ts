export type GitMode = "none" | "local" | "connected";

export interface GitCapabilities {
  can_commit: boolean;
  can_browse_history: boolean;
  can_reset_to_commit: boolean;
  can_switch_branch: boolean;
  can_diff: boolean;
  can_push: boolean;
  can_pull: boolean;
  can_fetch: boolean;
  can_force_push: boolean;
  can_rebase: boolean;
  can_open_pr: boolean;
  auto_feature_branch_on_protected: boolean;
}

export interface Workspace {
  id: string;
  name: string;
  workspace_id: string;
  active_branch: WorkspaceBranch | null;
  created_at: string;
  updated_at: string;

  workspace_error?: string;
  git_mode: GitMode;
  capabilities: GitCapabilities;
  default_branch: string;
  protected_branches: string[];

  /** True when this workspace is in local mode and has no config.yml yet. */
  requires_local_setup?: boolean;
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
