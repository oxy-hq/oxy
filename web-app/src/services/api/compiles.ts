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

const BASE = "/admin/compiles";

export const CompilesService = {
  async list(params: {
    limit?: number;
    workspace_id?: string;
    status?: string;
  }): Promise<ListCompilesResponse> {
    const res = await apiClient.get<ListCompilesResponse>(`${BASE}/`, {
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
  }
};
