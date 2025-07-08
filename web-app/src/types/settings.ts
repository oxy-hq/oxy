export interface CommitInfo {
  sha: string;
  message: string;
  author_name: string;
  author_email: string;
  date: string;
}

export interface RevisionInfo {
  current_revision?: string;
  latest_revision?: string;
  current_commit?: CommitInfo;
  latest_commit?: CommitInfo;
  sync_status: string;
  last_sync_time?: string;
}

export interface GitHubSettings {
  token_configured: boolean;
  selected_repo_id?: number;
  repository_name?: string;
}
