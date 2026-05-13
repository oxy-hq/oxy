export interface RevisionInfo {
  current_revision?: string;
  latest_revision?: string;
  current_commit?: string;
  latest_commit?: string;
  ahead_count: number;
  behind_count: number;
  uncommitted_count: number;
  is_in_conflict: boolean;
  last_sync_time?: string;
  remote_url?: string;
}
