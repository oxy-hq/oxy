import { apiClient } from "./axios";

export interface CompileEnqueueResponse {
  task_id: string;
  workspace_id: string;
  branch: string;
}

export interface RevisionSummary {
  revision_id: string;
  status: string;
  kind: string;
  branch: string | null;
  /** SHA the revision was compiled against. Compared to `head_sha`
   * to label the button as up-to-date vs stale. */
  git_sha: string;
  started_at: string;
  finished_at: string | null;
  duration_ms: number | null;
}

export interface CompileStatus {
  workspace_id: string;
  current_revision_id: string | null;
  latest: RevisionSummary | null;
  can_compile: boolean;
  /** HEAD commit on the workspace's default branch. Null for blank /
   * demo / no-remote workspaces. Compare against `latest.git_sha` to
   * label the button as up-to-date or stale. */
  head_sha: string | null;
  /** Workspace's default branch (`main` / `master` / custom). Null
   * matches `head_sha = null`. */
  default_branch: string | null;
}

export const CompileService = {
  async enqueue(workspaceId: string, branch: string): Promise<CompileEnqueueResponse> {
    const res = await apiClient.post<CompileEnqueueResponse>(`/${workspaceId}/compile`, undefined, {
      params: { branch }
    });
    return res.data;
  },

  async status(workspaceId: string, branch: string): Promise<CompileStatus> {
    const res = await apiClient.get<CompileStatus>(`/${workspaceId}/compile/status`, {
      params: { branch }
    });
    return res.data;
  }
};
