/** Mirrors `agentic_schedules` (backend `schedule::Model`, serde snake_case). */
export type ScheduleTargetKind = "workflow" | "airway" | "agent" | "monitor_scan";

export interface Schedule {
  id: string;
  project_id: string | null;
  branch_id: string | null;
  name: string;
  target_kind: ScheduleTargetKind;
  /** workflow_ref / pipeline_ref / agent_id, workspace-relative. */
  target_ref: string;
  /** Required when `target_kind === "agent"`, null otherwise. */
  question: string | null;
  variables: Record<string, unknown> | null;
  cron_expr: string;
  timezone: string;
  enabled: boolean;
  /** ISO 8601. */
  next_run_at: string;
  last_fired_at: string | null;
  last_run_id: string | null;
  /** Most recent fire/seed/cron failure; null once a fire succeeds. */
  last_error: string | null;
  /**
   * Cumulative count of cron occurrences that were silently skipped
   * because the scheduler tick didn't run in time (server downtime,
   * deploy gap). Policy is "run-once-then-resume" — only the first
   * missed slot fires; everything else past it is counted here.
   * Never decremented; purely audit.
   */
  missed_runs: number;
  /**
   * Timestamp (ISO 8601) of the most recent tick that detected a
   * catch-up. `null` when `missed_runs == 0`.
   */
  last_missed_at: string | null;
  created_at: string;
  updated_at: string;
}

/** Create/update payload (backend `ScheduleInput`). */
export interface ScheduleInput {
  name: string;
  target_kind: ScheduleTargetKind;
  target_ref: string;
  /** Required when `target_kind === "agent"`. Backend rejects an empty
   *  question for agent schedules; ignored for workflow / airway. */
  question?: string | null;
  variables?: Record<string, unknown> | null;
  cron_expr: string;
  /** Defaults to "UTC" server-side. */
  timezone?: string;
  /** Defaults to true server-side. */
  enabled?: boolean;
}

export interface RunNowResponse {
  run_id: string;
}

/** Body of `POST /agentic-schedules/:id/backfill`. */
export interface BackfillInput {
  /** Inclusive lower bound (ISO 8601 / RFC 3339). */
  from: string;
  /** Inclusive upper bound (ISO 8601 / RFC 3339); must be > `from`. */
  to: string;
  /** Throttle hint: `"sequential"` | `"<N>"` | `"all"`. Advisory in v1. */
  concurrency?: string;
}

export interface BackfillResponse {
  run_ids: string[];
  /** Cron occurrences enumerated; may exceed `run_ids.length` on partial seed. */
  planned: number;
}
