import { apiClient } from "./axios";

export interface ExplorerThread {
  id: string;
  title: string;
  input_snippet: string;
  source_type: string;
  is_processing: boolean;
  created_at: string;
  user_email: string | null;
  workspace_id: string | null;
  workspace_name: string | null;
  org_id: string | null;
  org_name: string | null;
  org_slug: string | null;
}

export interface ExplorerRun {
  id: string;
  question_snippet: string;
  task_status: string | null;
  source_type: string | null;
  error_message: string | null;
  created_at: string;
  thread_id: string | null;
  workspace_id: string | null;
  workspace_name: string | null;
  org_id: string | null;
  org_name: string | null;
  org_slug: string | null;
  user_email: string | null;
}

/** A page of explorer rows, with enough metadata to render pagination. */
export interface ExplorerPage<T> {
  items: T[];
  total: number;
  page: number;
  page_size: number;
}

export interface ExplorerQueryParams {
  search?: string;
  /** Resource-specific: run `task_status`, or "live"/"done" for threads. */
  status?: string;
  sourceType?: string;
  /** Scope results to one tenant's workspaces. Omit for the cross-tenant view. */
  orgId?: string;
  /** 1-indexed. */
  page?: number;
  pageSize?: number;
}

export const AdminExplorerService = {
  async threads(params: ExplorerQueryParams = {}): Promise<ExplorerPage<ExplorerThread>> {
    const res = await apiClient.get<ExplorerPage<ExplorerThread>>("/admin/explorer/threads", {
      params: {
        search: params.search,
        status: params.status,
        source_type: params.sourceType,
        org_id: params.orgId,
        page: params.page,
        page_size: params.pageSize
      }
    });
    return res.data;
  },
  async runs(params: ExplorerQueryParams = {}): Promise<ExplorerPage<ExplorerRun>> {
    const res = await apiClient.get<ExplorerPage<ExplorerRun>>("/admin/explorer/runs", {
      params: {
        search: params.search,
        status: params.status,
        source_type: params.sourceType,
        org_id: params.orgId,
        page: params.page,
        page_size: params.pageSize
      }
    });
    return res.data;
  }
};
