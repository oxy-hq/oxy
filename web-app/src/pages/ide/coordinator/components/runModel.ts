import type { ActiveRunEntry, AnomalyInfo, RunHistoryEntry } from "@/services/api/coordinator";
import {
  isSystemSource,
  type JobType,
  normalizeStatus,
  normalizeTrigger,
  type RunStatus,
  sourceTypeToJobType,
  type Trigger
} from "./constants";

/**
 * A run normalized into the shape every coordinator surface speaks. The two
 * backend payloads (active runs + run history) differ slightly; this is the
 * common `{ id, status, start, end, trigger }` abstraction the polymorphic
 * run-detail page and the timeline both build on.
 */
export interface NormalizedRun {
  runId: string;
  status: RunStatus;
  jobType: JobType;
  /** The user question / job label. */
  title: string;
  /** Agent id, falling back to the source type. */
  subtitle: string;
  source: string;
  attempt: number;
  startedAt: string;
  /** null while the run is still in flight. */
  endedAt: string | null;
  live: boolean;
  /** Soft FK to the schedule that produced this run, when applicable. */
  scheduleId: string | null;
  /** Which path triggered this run — null for legacy runs without the tag. */
  trigger: Trigger | null;
  /** Server-flagged "healthy but weird" — duration/cost/row anomalies. */
  anomaly: AnomalyInfo | null;
  /** Estimated USD cost of this run's LLM calls; null for non-LLM runs
   *  and for runs whose model isn't in the pricing table. */
  costUsd: number | null;
  /** Total tokens across all LLM calls on this run; null for non-LLM
   *  runs. Independent of `costUsd` so a model missing from the pricing
   *  table still shows usage. */
  tokensTotal: number | null;
  /** True when this run is a system-managed daemon (e.g. preagg heartbeat)
   *  rather than user-scheduled work. Drives the SystemBadge in lists. */
  isSystem: boolean;
  errorMessage?: string;
}

const buildRun = (r: ActiveRunEntry | RunHistoryEntry, errorMessage?: string): NormalizedRun => {
  const status = normalizeStatus(r.status);
  const live = status === "running" || status === "suspended";
  return {
    runId: r.run_id,
    status,
    jobType: sourceTypeToJobType(r.source_type),
    title: r.question || "(untitled run)",
    subtitle: r.agent_id || r.source_type || "—",
    source: r.source_type || "—",
    attempt: r.attempt,
    startedAt: r.created_at,
    endedAt: live ? null : r.updated_at,
    live,
    scheduleId: r.schedule_id ?? null,
    trigger: normalizeTrigger(r.trigger),
    anomaly: r.anomaly ?? null,
    costUsd: r.cost_usd ?? null,
    tokensTotal: r.tokens_total ?? null,
    isSystem: isSystemSource(r.source_type),
    errorMessage
  };
};

export const normalizeActiveRun = (r: ActiveRunEntry): NormalizedRun => buildRun(r);

export const normalizeHistoryRun = (r: RunHistoryEntry): NormalizedRun =>
  buildRun(r, r.error_message);

/**
 * Merge active runs and run history into one de-duplicated, newest-first
 * list. Active runs win on conflict — their status is fresher.
 */
export const mergeRuns = (
  active: ActiveRunEntry[],
  history: RunHistoryEntry[]
): NormalizedRun[] => {
  const byId = new Map<string, NormalizedRun>();
  for (const r of history) byId.set(r.run_id, normalizeHistoryRun(r));
  for (const r of active) byId.set(r.run_id, normalizeActiveRun(r));
  return [...byId.values()].sort(
    (a, b) => new Date(b.startedAt).getTime() - new Date(a.startedAt).getTime()
  );
};
