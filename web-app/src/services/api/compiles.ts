import { apiClient } from "./axios";

/**
 * One row in the recent-compiles list. Mirrors the backend's
 * `admin::compiles::CompileRow`.
 */
export interface CompileRow {
  revision_id: string;
  workspace_id: string;
  git_sha: string;
  branch: string | null;
  status: "compiling" | "ready" | "failed" | string;
  kind: "main" | "draft" | string;
  owner_user_id: string | null;
  compiler_version: string;
  schema_version: number;
  started_at: string;
  finished_at: string | null;
  duration_ms: number | null;
  file_count_seen: number;
  file_count_compiled: number;
  file_count_failed: number;
  is_current_for_workspace: boolean;
}

export interface CompileFailure {
  path: string;
  kind: "yaml" | "io" | "shape" | "duplicate" | string;
  message: string;
}

export interface CompileDetail extends CompileRow {
  error_summary: { failures?: CompileFailure[]; fatal?: string } | null;
}

export interface ListCompilesResponse {
  rows: CompileRow[];
  total_returned: number;
}

export interface RunCompileRequest {
  workspace_id: string;
  git_sha?: string;
  branch?: string;
  promote?: boolean;
}

export interface RunCompileResponse {
  task_id: string;
  workspace_id: string;
  promote: boolean;
}

export interface BackfillResponse {
  enqueued: number;
  /** True when the batch cap was hit and more uncompiled workspaces remain. */
  remaining: boolean;
  task_ids: string[];
}

export interface PromoteResponse {
  revision_id: string;
  workspace_id: string;
}

const BASE = "/admin/compiles";

export const CompilesService = {
  async list(params: {
    limit?: number;
    workspace_id?: string;
    status?: string;
  }): Promise<ListCompilesResponse> {
    const res = await apiClient.get<ListCompilesResponse>(BASE, {
      params
    });
    return res.data;
  },

  async detail(revisionId: string): Promise<CompileDetail> {
    const res = await apiClient.get<CompileDetail>(`${BASE}/${encodeURIComponent(revisionId)}`);
    return res.data;
  },

  async runNow(request: RunCompileRequest): Promise<RunCompileResponse> {
    const res = await apiClient.post<RunCompileResponse>(`${BASE}/run`, request);
    return res.data;
  },

  /** Enqueue a promoting compile for every workspace that has never been compiled. */
  async backfillUncompiled(): Promise<BackfillResponse> {
    const res = await apiClient.post<BackfillResponse>(`${BASE}/backfill`);
    return res.data;
  },

  /**
   * Rollback lever: repoint the revision's workspace at this (ready, main)
   * revision — used to revert a bad compile to a known-good prior one.
   */
  async promote(revisionId: string): Promise<PromoteResponse> {
    const res = await apiClient.post<PromoteResponse>(
      `${BASE}/${encodeURIComponent(revisionId)}/promote`
    );
    return res.data;
  }
};
