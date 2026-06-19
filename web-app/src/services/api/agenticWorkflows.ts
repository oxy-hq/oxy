/**
 * Client for the new `/agentic-workflows` HTTP surface (mounted per workspace).
 *
 * Native JSON in/out — no `OutputContainer` translation. Events are
 * forwarded by the runtime registry as `(event_type, payload)` JSON pairs;
 * the SSE helper here just re-emits them.
 */

import { fetchEventSource } from "@microsoft/fetch-event-source";

import { apiBaseURL } from "../env";
import { apiClient } from "./axios";

// ── Wire types ─────────────────────────────────────────────────────────────

export type StartWorkflowRequest = {
  workflow_ref: string;
  variables?: Record<string, unknown>;
  retry_from_run_id?: string;
  /** Default off — only consulted when `retry_from_run_id` is set. */
  cache_enabled?: boolean;
  /**
   * Step names to force-invalidate on this retry. The decider re-runs
   * each named step (cache-miss) and the cascade naturally re-runs
   * everything downstream because their `render_context` changes.
   *
   * Used by the diagram's per-step "Replay" button: pass the clicked
   * step name and the run id of the prior run as `retry_from_run_id`
   * with `cache_enabled: true`.
   */
  invalidate_steps?: string[];
  /**
   * Per-step iteration indices to force-replay on this retry, ignoring
   * the per-iteration cache. Map of step name → indices.
   *
   * Used by the Retry popover's iteration grid: clicking a specific
   * iteration to override pushes its index here. The decider applies
   * this alongside the prior-run cache lookup; listed `(step, index)`
   * pairs always re-delegate, others reuse the prior result when
   * `cache_enabled` is `true`.
   */
  invalidate_iterations?: Record<string, number[]>;
  /**
   * Optional thread association. When set, `agentic_runs.thread_id` is
   * filled in so the chat-thread workflow page can recover state on
   * reload — without it, the in-memory log buffer is the only source of
   * truth and the page goes blank after refresh.
   */
  thread_id?: string;
};

/**
 * One entry in a loop step's `iterations` map. Each loop step's
 * aggregated result carries `{ "iterations": { "<hash>": IterationOutcome, ... } }`
 * — see `agentic-workflow::step_decider::iteration_hash`.
 */
export type IterationOutcome = {
  /** Iteration value (as serialized by the workflow YAML). */
  value: unknown;
  /** 0-based position in the loop's `values:` array at the time of the run. */
  index: number;
  /**
   * Snapshot-derived outcomes (Retry popover) carry the three
   * terminal states; live values (loop progress bar feeding the
   * sidebar grid) can additionally be `"running"` while an iteration
   * is in flight.
   */
  status: "done" | "failed" | "cancelled" | "running";
  /** Set when `status === "done"`. */
  answer?: unknown;
  /** Set when `status === "failed"`. */
  error?: string;
};

export type WorkflowRunSnapshot = {
  run_id: string;
  workflow_ref: string | null;
  status: string;
  error_message: string | null;
  answer: string | null;
  current_step: number;
  total_steps: number;
  /** step_name → result JSON for completed steps. */
  results: Record<string, unknown>;
  /** step_name → SHA-256 hex for steps that succeeded. */
  step_hashes: Record<string, string>;
  retry_from_run_id: string | null;
  cache_enabled: boolean;
};

export type WorkflowFile = {
  path: string;
  path_b64: string;
  /**
   * Top-level `description` from the workflow YAML, when present.
   * Trimmed and truncated server-side (~200 chars); `null`/absent for
   * files without one or that failed to read/parse.
   */
  description?: string | null;
};

export type WorkflowFileContent = {
  path: string;
  content: string;
};

export type WorkflowRunSummary = {
  run_id: string;
  status: string;
  /** ISO 8601 string from the backend (`chrono::DateTime`). */
  created_at: string;
  updated_at: string;
};

/**
 * Recursive task-DAG descriptor carried in the `subrun_started` payload.
 *
 * Container task types populate `inner_tasks` so renderers can walk the
 * full nested tree from a single event:
 *  - `loop_sequential` — the loop body's task definitions.
 *  - `workflow` (sub-workflow) — the referenced child workflow's tasks,
 *    pre-resolved server-side (recursive). Empty when the child file is
 *    missing or appears in a cycle.
 *  - all others — omitted.
 */
export type SubrunStepInfo = {
  name: string;
  task_type: string;
  inner_tasks?: SubrunStepInfo[];
};

/** Frontend-facing event type matching the SSE payload from agentic-runtime. */
export type WorkflowEvent =
  | {
      type: "subrun_started";
      payload: {
        subrun_name: string;
        steps: SubrunStepInfo[];
      };
    }
  | { type: "subrun_step_started"; payload: { step: string } }
  | {
      type: "subrun_step_cache_hit";
      payload: {
        step: string;
        /**
         * Set when the hit comes from the step-hash cache (i.e. the
         * decider matched this step's hash against a `prior_run`).
         * Absent when `source === "file"`.
         */
        prior_run_id?: string;
        /**
         * `"file"` when the step short-circuited because a file
         * existed at the configured `cache.path`. Absent for the
         * step-hash cache. The two paths surface as the same event
         * type so the FE can render either case.
         */
        source?: "file";
        /**
         * Workspace-relative path of the cache file, when
         * `source === "file"`. Absent otherwise.
         */
        path?: string;
      };
    }
  | {
      type: "subrun_step_output";
      payload: {
        step: string;
        task_type: string;
        /** Task-type-specific JSON shape — see WorkflowStepOutput in the FE. */
        output: unknown;
      };
    }
  | {
      type: "subrun_step_completed";
      payload: {
        step: string;
        success: boolean;
        cached?: boolean;
        error?: string;
      };
    }
  | {
      type: "subrun_completed";
      payload: { subrun_name: string; success: boolean };
    }
  /**
   * Loop-iteration lifecycle events. Emitted by the workflow decider
   * when a `loop_sequential` step fans out:
   *   - `started` per delegated iteration, just before the children
   *     are assigned to workers. Cache-hit iterations are *not*
   *     emitted — they appear directly in the snapshot's
   *     `iterations` map at decide time.
   *   - `completed` per fresh iteration outcome folded back into
   *     the loop step's result (any number of these can land in a
   *     single `decide()` call when the coordinator aggregates).
   *
   * The FE uses these to drive the live progress bar on the loop's
   * diagram node — see `RunStepState.iterations`.
   */
  | {
      type: "subrun_step_iteration_started";
      payload: { step: string; index: number; value: unknown };
    }
  | {
      type: "subrun_step_iteration_completed";
      payload: {
        step: string;
        index: number;
        status: "done" | "failed" | "cancelled";
        error?: string;
      };
    }
  /**
   * Coordinator-emitted: a worker reported `Failed` for a task. Fires
   * once per attempt, *before* the coordinator decides retry / fallback
   * / finalize — admins can attribute every failure to the specific
   * `spec_kind` that errored. Subsequent events on the same stream
   * (`delegation_retry`, `delegation_fallback`, `delegation_completed`)
   * narrate the coordinator's reaction.
   */
  | {
      type: "task_failed";
      payload: {
        task_id: string;
        attempt: number;
        /** `agent` / `workflow` / `workflow_step` / `workflow_decision` / `resume` */
        spec_kind: string | null;
        /**
         * For `spec_kind === "workflow_step"` only — the failing
         * step's name (pulled off `step_config["name"]`). Lets admins
         * jump directly from the trace event to the offending row in
         * the Output view. `null` for other spec kinds.
         */
        step_name: string | null;
        error: string;
      };
    }
  /**
   * Worker-emitted: a task was just claimed off the queue. Pairs with
   * the eventual `task_failed` / `delegation_completed` event to give
   * admins a wall-clock claim → outcome window, distinguishing "stuck
   * in the queue" from "stuck executing."
   */
  | {
      type: "worker_task_claimed";
      payload: {
        task_id: string;
        spec_kind: string;
        parent_task_id: string | null;
      };
    }
  /**
   * Coordinator-emitted: a parent task transitioned to "waiting on
   * children" after a (single or parallel) fan-out. Fires once per
   * transition with the full child list and the failure policy that
   * decides whether one child's failure aborts the fan-out.
   *
   * Lets an admin tree view render the fan-out boundary as a single
   * node instead of inferring it from a cluster of `delegation_started`
   * events.
   */
  | {
      type: "waiting_on_children";
      payload: {
        parent_task_id: string;
        child_task_ids: string[];
        /** Raw `FanoutFailurePolicy` serialisation; structure may evolve. */
        failure_policy: unknown;
      };
    }
  /**
   * Decider trace: emitted before each `WorkflowDecision`'s own events
   * so admins can see which variant the decider picked and the relevant
   * shape (step index/name, fan-out item count, target kind, etc.).
   *
   * `payload.kind` discriminates the WorkflowDecision variant.
   * Additional fields are variant-specific — frontend code should treat
   * the payload as `Record<string, unknown>` and switch on `kind`.
   */
  | {
      type: "decider_decided";
      payload: {
        kind:
          | "delegate_step"
          | "delegate_parallel"
          | "step_executed_inline"
          | "wait_for_more_children"
          | "complete"
          | "fail";
        step_index?: number;
        step_name?: string;
        item_count?: number;
        targets?: string[];
        target?: { kind: string; agent_id?: string; workflow_ref?: string };
        failure_policy?: unknown;
        error?: string;
      };
    };

// ── Service ────────────────────────────────────────────────────────────────

export class AgenticWorkflowService {
  private static base(projectId: string): string {
    return `/${projectId}/agentic-workflows`;
  }

  static async listFiles(projectId: string): Promise<WorkflowFile[]> {
    // The procedure LIST is served from the compile boundary at `/procedures`
    // (FleetOk), so the customer-nav sidebar renders on a stateless serve
    // replica with no working copy. The single-file fetch (`getFile`) stays on
    // `/agentic-workflows/files/:path_b64` (IdeOnly) — it reads the working copy.
    const { data } = await apiClient.get(`/${projectId}/procedures`);
    return data;
  }

  static async getFile(projectId: string, pathB64: string): Promise<WorkflowFileContent> {
    // Served from the compile boundary at `/procedures/{path_b64}` (FleetOk) so a
    // procedure's diagram renders on a stateless serve replica; the legacy
    // `/agentic-workflows/files/{path_b64}` route is IdeOnly (proxied → 502 when
    // the ide is down).
    const { data } = await apiClient.get(`/${projectId}/procedures/${pathB64}`);
    return data;
  }

  static async startRun(
    projectId: string,
    request: StartWorkflowRequest
  ): Promise<{ run_id: string }> {
    const { data } = await apiClient.post(
      `${AgenticWorkflowService.base(projectId)}/runs`,
      request
    );
    return data;
  }

  static async getRun(projectId: string, runId: string): Promise<WorkflowRunSnapshot> {
    const { data } = await apiClient.get(`${AgenticWorkflowService.base(projectId)}/runs/${runId}`);
    return data;
  }

  static async listRuns(
    projectId: string,
    workflowRef: string,
    limit = 50
  ): Promise<WorkflowRunSummary[]> {
    const { data } = await apiClient.get(`${AgenticWorkflowService.base(projectId)}/runs`, {
      params: { workflow_ref: workflowRef, limit }
    });
    return data;
  }

  /**
   * Return the latest workflow run linked to this thread, or `null` if
   * none exists yet. Used by the chat-thread workflow page on reload to
   * resume from `agentic_runs.thread_id` instead of the lost zustand
   * log buffer.
   */
  static async latestRunForThread(
    projectId: string,
    threadId: string
  ): Promise<{ run_id: string; task_status: string | null } | null> {
    try {
      const { data } = await apiClient.get(
        `${AgenticWorkflowService.base(projectId)}/threads/${threadId}/run`
      );
      return data;
    } catch (e) {
      // 404 just means "this thread never ran a workflow"; bubble other
      // statuses so the page can show a loading-error state if needed.
      const status = (e as { response?: { status?: number } })?.response?.status;
      if (status === 404) return null;
      throw e;
    }
  }

  static async cancelRun(projectId: string, runId: string): Promise<void> {
    await apiClient.post(`${AgenticWorkflowService.base(projectId)}/runs/${runId}/cancel`);
  }

  /**
   * Stream events for a run. `onEvent` is called for each event type the
   * backend emits. Returns when the stream closes (workflow terminal) or
   * when the supplied `signal` aborts.
   *
   * The backend uses the SSE `event:` field for the type (`subrun_started`,
   * `subrun_step_started`, etc.) and `data:` for the payload — not a
   * single `{type, payload}` JSON envelope. The shared `fetchSSE` helper
   * only forwards `data`, so we read raw `EventSourceMessage`s here.
   */
  static async streamEvents(
    projectId: string,
    runId: string,
    options: {
      onEvent: (event: WorkflowEvent) => void;
      onOpen?: () => void;
      onClose?: () => void;
      onError?: (error: Error) => void;
      signal?: AbortSignal;
    }
  ): Promise<void> {
    const url = `${apiBaseURL}${AgenticWorkflowService.base(projectId)}/runs/${runId}/events`;
    const known = new Set<WorkflowEvent["type"]>([
      "subrun_started",
      "subrun_step_started",
      "subrun_step_cache_hit",
      "subrun_step_output",
      "subrun_step_completed",
      "subrun_completed",
      // Loop iteration lifecycle — drives the in-node progress bar
      // on loop_sequential steps via RunStepState.iterations.
      "subrun_step_iteration_started",
      "subrun_step_iteration_completed",
      // Coordinator/worker trace events surfaced in the Trace tab.
      // The backend emits these via the EventRegistry (see
      // crates/agentic/runtime/src/event_registry.rs) and the
      // WorkflowEvent union above already declares their shapes;
      // omitting them here filters them out before they reach the
      // reducer, which is why the Trace tab used to render the same
      // subrun_* rows the Output tab already shows.
      "task_failed",
      "worker_task_claimed",
      "waiting_on_children",
      "decider_decided"
    ]);
    const token = localStorage.getItem("auth_token");
    await fetchEventSource(url, {
      method: "GET",
      headers: { Authorization: token ?? "" },
      openWhenHidden: true,
      signal: options.signal,
      async onopen(res) {
        if (res.status !== 200) {
          throw new Error(`SSE connection failed with status: ${res.status}`);
        }
        options.onOpen?.();
      },
      onmessage(ev) {
        // Drop heartbeats and unknown event types — keeps the UI surface
        // tight, matches the explicit list known() iterates over.
        if (!ev.event || !known.has(ev.event as WorkflowEvent["type"])) return;
        try {
          const payload = JSON.parse(ev.data);
          options.onEvent({
            type: ev.event as WorkflowEvent["type"],
            payload
          } as WorkflowEvent);
        } catch (error) {
          console.error("Error parsing SSE data:", error);
        }
      },
      onerror(err) {
        console.error("SSE error:", err);
        options.onError?.(err);
        throw err;
      },
      onclose() {
        options.onClose?.();
      }
    });
  }
}
