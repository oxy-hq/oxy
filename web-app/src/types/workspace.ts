import type { WorkspaceRole } from "@/types/organization";

type GitMode = "none" | "local" | "connected";

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

  /** Authenticated user's effective role in this workspace. Optional so
   * existing callers that build a partial Workspace don't have to fabricate
   * a value; consumers that need the role should fall back to "viewer". */
  current_user_role?: WorkspaceRole;

  /** Namespace for per-workspace browser state (onboarding wizard
   * localStorage). UUID in cloud; `local:{path-hash}` in local — see
   * `compute_workspace_storage_key` in the Rust side for the contract.
   * Optional only because some legacy callers fabricate partial
   * Workspaces; real server responses always populate it. */
  storage_key?: string;
}

type BranchOrigin = "local_only" | "remote_only" | "both";

export interface WorkspaceBranch {
  name: string;
  revision: string;
  id: string;
  created_at: string;
  updated_at: string;
  branch_type: "local" | "remote";
  /** Where this branch lives. Drives badges + the switch flow:
   *  `remote_only` requires the server to create a local tracking branch
   *  before the worktree can be created. */
  origin: BranchOrigin;
}

export interface WorkspaceBranchesResponse {
  branches: WorkspaceBranch[];
}
