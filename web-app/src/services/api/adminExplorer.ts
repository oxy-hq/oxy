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

export const AdminExplorerService = {
  async threads(search: string, limit = 50): Promise<ExplorerThread[]> {
    const res = await apiClient.get<ExplorerThread[]>("/admin/explorer/threads", {
      params: { search, limit }
    });
    return res.data;
  },
  async runs(search: string, status = "", limit = 50): Promise<ExplorerRun[]> {
    const res = await apiClient.get<ExplorerRun[]>("/admin/explorer/runs", {
      params: { search, status, limit }
    });
    return res.data;
  }
};
