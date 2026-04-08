export interface Repository {
  name: string;
  path?: string;
  git_url?: string;
  branch?: string;
  git_namespace_id?: string;
}

export interface AddRepositoryRequest {
  name: string;
  path?: string;
  git_url?: string;
  branch?: string;
}

export interface AddRepositoryFromGitHubRequest {
  name: string;
  git_namespace_id: string;
  clone_url: string;
  branch?: string;
}
