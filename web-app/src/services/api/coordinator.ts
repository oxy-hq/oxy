import { apiBaseURL } from "../env";
import { apiClient } from "./axios";

export interface ActiveRunEntry {
  run_id: string;
  status: "running" | "suspended" | "done" | "failed" | "cancelled";
  question: string;
  agent_id: string;
  source_type: string;
  attempt: number;
  created_at: string;
  updated_at: string;
}

export interface ActiveRunsResponse {
  runs: ActiveRunEntry[];
  total: number;
}

export interface RunHistoryEntry {
  run_id: string;
  status: string;
  question: string;
  agent_id: string;
  source_type: string;
  answer?: string;
  error_message?: string;
  attempt: number;
  created_at: string;
  updated_at: string;
}

export interface RunHistoryResponse {
  runs: RunHistoryEntry[];
  total: number;
}

export interface TaskTreeNode {
  run_id: string;
  parent_run_id: string | null;
  status: string;
  question: string;
  agent_id: string;
  source_type: string;
  answer?: string;
  error_message?: string;
  attempt: number;
  task_status?: string;
  created_at: string;
  updated_at: string;
  outcome_status?: string;
}

export interface TaskTreeResponse {
  root_id: string;
  nodes: TaskTreeNode[];
}

export interface AgentStats {
  agent_id: string;
  total: number;
  succeeded: number;
  failed: number;
  recovered: number;
}

export interface RecoveredRunEntry {
  run_id: string;
  status: string;
  question: string;
  agent_id: string;
  attempt: number;
  created_at: string;
  updated_at: string;
}

export interface RecoveryResponse {
  total_runs: number;
  recovered_count: number;
  failed_count: number;
  cancelled_count: number;
  succeeded_count: number;
  agents: AgentStats[];
  recovered_runs: RecoveredRunEntry[];
}

export interface QueueTaskEntry {
  task_id: string;
  run_id: string;
  queue_status: string;
  worker_id?: string;
  claim_count: number;
  max_claims: number;
  last_heartbeat?: string;
  created_at: string;
  updated_at: string;
}

export interface QueueHealthResponse {
  queued: number;
  claimed: number;
  completed: number;
  failed: number;
  cancelled: number;
  dead: number;
  stale_tasks: QueueTaskEntry[];
  dead_tasks: QueueTaskEntry[];
}

export interface LiveStatusEntry {
  run_id: string;
  status: string;
}

export class CoordinatorService {
  static async getActiveRuns(projectId: string): Promise<ActiveRunsResponse> {
    const response = await apiClient.get(`/${projectId}/analytics/coordinator/active-runs`);
    return response.data;
  }

  static async getRunHistory(
    projectId: string,
    params: {
      limit?: number;
      offset?: number;
      status?: string;
      source_type?: string;
    } = {}
  ): Promise<RunHistoryResponse> {
    const response = await apiClient.get(`/${projectId}/analytics/coordinator/runs`, {
      params: { limit: params.limit ?? 25, ...params }
    });
    return response.data;
  }

  static async getRunTree(projectId: string, runId: string): Promise<TaskTreeResponse> {
    const response = await apiClient.get(`/${projectId}/analytics/coordinator/runs/${runId}/tree`);
    return response.data;
  }

  static async getRecoveryStats(projectId: string, limit = 200): Promise<RecoveryResponse> {
    const response = await apiClient.get(`/${projectId}/analytics/coordinator/recovery`, {
      params: { limit }
    });
    return response.data;
  }

  static async getQueueHealth(projectId: string): Promise<QueueHealthResponse> {
    const response = await apiClient.get(`/${projectId}/analytics/coordinator/queue`);
    return response.data;
  }

  static liveStreamUrl(projectId: string): string {
    return `${apiBaseURL}/${projectId}/analytics/coordinator/live`;
  }
}
