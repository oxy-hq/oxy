import { apiClient } from "./axios";

export type WorkspaceHealthStatus = "healthy" | "degraded" | "unhealthy";

export type WorkspaceHealthDimensionKey =
  | "job_liveness"
  | "pipeline"
  | "correctness"
  | "queue"
  | "reconciliation";

export interface WorkspaceHealthDimension {
  dimension: WorkspaceHealthDimensionKey;
  status: WorkspaceHealthStatus;
  reason: string | null;
}

/**
 * One reconciliation drift check: an `actual` operand compared against an
 * `expected` reference (each a semantic query, SQL, external source, or constant).
 * Numeric fields are `null` when the source was unreachable or errored (the backend
 * stores NaN/Infinity there, which serialize to JSON null).
 */
export interface WorkspaceHealthReconciliationCheck {
  check: string;
  /** Friendly check-level text; null when the check omitted `description`. */
  description: string | null;
  /** Backend-resolved label for the actual operand ("Actual" when unset). */
  actual_label: string;
  /** Backend-resolved label for the expected operand ("Expected" when unset). */
  expected_label: string;
  actual: number | null;
  expected: number | null;
  abs_diff: number | null;
  /** Percent drift relative to the expected (reference) value, in percent units (3.0 == 3%). */
  pct_diff: number | null;
  status: WorkspaceHealthStatus;
  reason: string | null;
}

export interface WorkspaceHealthSignals {
  failed_runs: number;
  timed_out_runs: number;
  total_runs: number;
  airway_last_run_failed: boolean;
  airway_completed_with_errors: boolean;
  open_high_anomalies: number;
  open_medium_anomalies: number;
  dead_letter_count: number;
}

export interface WorkspaceHealthEntry {
  workspace_id: string;
  /** Workspace display name; null only if the workspace row was deleted mid-scan. */
  workspace_name: string | null;
  /** Owning org name; null when the workspace has no org (or was deleted). */
  org_name: string | null;
  status: WorkspaceHealthStatus;
  reasons: string[];
  dimensions: WorkspaceHealthDimension[];
  /**
   * Raw signal counts. `null` right after the payload migration backfills NULL
   * (before the next sweep writes a full payload) — render a fallback, never
   * index into it unguarded.
   */
  signals: WorkspaceHealthSignals | null;
  /** Per-check reconciliation drift detail; empty when the workspace has no `reconcile.yml`. */
  reconciliation: WorkspaceHealthReconciliationCheck[];
  /** ISO timestamp of the last status transition; null until the first eval pass records it. */
  changed_at: string | null;
  /** ISO timestamp of the last eval-pass sweep ("last checked"), refreshed every pass even when status is unchanged; null until the first pass records it. */
  checked_at: string | null;
}

export interface WorkspaceHealthResponse {
  workspaces: WorkspaceHealthEntry[];
}

/**
 * Response to an on-demand eval trigger. The eval is enqueued onto the worker
 * fleet (HTTP 202) rather than run inline, so the trigger returns only the
 * `run_id`; the refreshed row is read back from the rollup once the pass lands.
 */
export interface TriggerEvalResponse {
  run_id: string;
}

export const WorkspaceHealthService = {
  async list(): Promise<WorkspaceHealthResponse> {
    const response = await apiClient.get<WorkspaceHealthResponse>("/admin/workspace-health");
    return response.data;
  },
  async trigger(workspaceId: string): Promise<TriggerEvalResponse> {
    const response = await apiClient.post<TriggerEvalResponse>(
      `/admin/workspace-health/${workspaceId}/eval`
    );
    return response.data;
  }
};
