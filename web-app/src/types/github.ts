export interface GitHubRepository {
  id: number;
  name: string;
  full_name: string;
  html_url: string;
  description?: string;
  default_branch: string;
  updated_at: string;
  clone_url: string;
}

export interface GitHubBranch {
  name: string;
}

export interface GitHubNamespace {
  id: string;
  owner_type: string;
  slug: string;
  name: string;
}

export interface GitHubNamespacesResponse {
  installations: GitHubNamespace[];
}

export interface CreateGitNamespaceRequest {
  installation_id: string;
  state: string;
  code: string;
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

export interface GitHubAppInstallationRequest {
  installation_id: string;
  // app_id field removed: we now use environment variables for app_id
}
