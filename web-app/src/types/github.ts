export interface GitHubRepository {
  id: number;
  name: string;
  full_name: string;
  html_url: string;
  description?: string;
  default_branch: string;
  updated_at: string;
}

export interface ProjectStatus {
  github_connected: boolean;
  repository?: GitHubRepository;
  required_secrets?: string[];
  is_config_valid: boolean;
  is_readonly: boolean;
  is_onboarded: boolean;
  repository_sync_status: RepositorySyncStatus | null;
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
