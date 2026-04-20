export interface RevisionInfo {
  current_revision?: string;
  latest_revision?: string;
  current_commit?: string;
  latest_commit?: string;
  sync_status: string;
  last_sync_time?: string;
  remote_url?: string;
}
