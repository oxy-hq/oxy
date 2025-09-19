export interface GitHubRepository {
  id: number;
  name: string;
  full_name: string;
  html_url: string;
  description?: string;
  default_branch: string;
  updated_at: string;
}

export interface GitHubBranch {
  name: string;
}

export interface ProjectStatus {
  required_secrets?: string[];
  is_config_valid: boolean;
  error?: string;
}

export type RepositorySyncStatus = "idle" | "syncing" | "synced" | "error";

export interface CurrentProject {
  repository?: GitHubRepository;
  local_path?: string;
  sync_status: ProjectSyncStatus;
}

export type ProjectSyncStatus =
  | "synced"
  | "pending"
  | { error: string }
  | "not_configured";

export interface StoreTokenRequest {
  token: string;
}

export interface TokenResponse {
  success: boolean;
  message: string;
}

export interface SelectRepositoryRequest {
  repository_id: number;
}

export interface SelectRepositoryResponse {
  success: boolean;
  message: string;
}

export interface ListRepositoriesResponse {
  repositories: GitHubRepository[];
}
