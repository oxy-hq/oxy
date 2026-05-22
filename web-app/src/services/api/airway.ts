/**
 * Client for the `/agentic-airway` HTTP surface (mounted per workspace).
 *
 * Airway is queue-driven like workflow: `startRun` seeds the run and
 * the runtime coordinator drives it; events stream over the shared
 * domain-agnostic SSE endpoint, routed by `source_type = "airway"`.
 * Mirrors `agenticWorkflows.ts` — same SSE shape (`event:` field for
 * the type, `data:` for the payload).
 */

import { fetchEventSource } from "@microsoft/fetch-event-source";

import { apiBaseURL } from "../env";
import { apiClient } from "./axios";

// ── Wire types ─────────────────────────────────────────────────────────────

export type StartAirwayRequest = {
  /** Path to a `.airway.yml`, relative to the workspace root. */
  pipeline_ref: string;
  /** Optional variables rendered into the pipeline YAML at run time. */
  variables?: Record<string, unknown>;
  /** Optional conversation-thread association. */
  thread_id?: string;
};

export type AirwayRunSummary = {
  run_id: string;
  status: string;
  /** ISO 8601 (`chrono::DateTime`). */
  created_at: string;
  updated_at: string;
};

/** `.airway.yml` file ref (parity with `WorkflowFile`). */
export type AirwayFile = {
  path: string;
  path_b64: string;
};

/**
 * Frontend mirror of `agentic_airway::AirwayEvent`. The backend tags
 * with `event_type` (snake_case) and the runtime splits that into the
 * SSE `event:` field; `data:` carries the full payload (the tag is
 * echoed inside it too, harmlessly).
 */
export type AirwayEvent =
  | {
      type: "load_started";
      payload: { pipeline_name: string; load_id: string };
    }
  | {
      type: "pipeline_plan";
      payload: {
        pipeline_name: string;
        load_id: string;
        resources: string[];
        destination: string;
      };
    }
  | {
      type: "extract_started";
      payload: { pipeline_name: string; load_id: string; table: string };
    }
  | {
      type: "extract_progress";
      payload: {
        pipeline_name: string;
        load_id: string;
        table: string;
        rows_so_far: number;
      };
    }
  | {
      type: "extract_completed";
      payload: {
        pipeline_name: string;
        load_id: string;
        table: string;
        rows_extracted: number;
      };
    }
  | {
      type: "normalize_started";
      payload: { pipeline_name: string; load_id: string; table: string };
    }
  | {
      type: "normalize_completed";
      payload: {
        pipeline_name: string;
        load_id: string;
        table: string;
        rows_normalized: number;
        child_tables: string[];
      };
    }
  | {
      type: "destination_load_started";
      payload: { pipeline_name: string; load_id: string; tables: string[] };
    }
  | {
      type: "table_load_started";
      payload: { pipeline_name: string; load_id: string; table: string };
    }
  | {
      type: "load_progress";
      payload: {
        pipeline_name: string;
        load_id: string;
        table: string;
        rows_written: number;
      };
    }
  | {
      type: "table_load_failed";
      payload: {
        pipeline_name: string;
        load_id: string;
        table: string;
        error: string;
      };
    }
  | {
      type: "table_loaded";
      payload: {
        pipeline_name: string;
        load_id: string;
        table: string;
        rows: number;
      };
    }
  | {
      type: "load_completed";
      payload: {
        pipeline_name: string;
        load_id: string;
        tables: string[];
        /** Per-table row counts written to the destination. */
        rows_loaded: Record<string, number>;
        duration_ms: number;
      };
    }
  | {
      type: "schema_evolved";
      payload: { pipeline_name: string; changes: unknown };
    }
  | {
      type: "resource_failed";
      payload: {
        pipeline_name: string;
        load_id: string;
        table: string;
        error: string;
      };
    }
  | {
      type: "pipeline_error";
      payload: {
        pipeline_name: string;
        load_id: string | null;
        error: string;
      };
    }
  | {
      type: "cancelled";
      payload: { pipeline_name: string; load_id: string };
    }
  // Coordinator-level failure (not an engine event): emitted when
  // `execute_airway` errors *before* the worker starts — secrets,
  // connector/destination resolution, config/spec issues. Without
  // handling this the run page stays empty on a pre-processing error.
  | {
      type: "task_failed";
      payload: {
        task_id?: string;
        attempt?: number;
        spec_kind?: string | null;
        step_name?: string | null;
        error: string;
      };
    };

export type AirwayEventType = AirwayEvent["type"];

const KNOWN_EVENTS = new Set<AirwayEventType>([
  "load_started",
  "pipeline_plan",
  "extract_started",
  "extract_progress",
  "extract_completed",
  "normalize_started",
  "normalize_completed",
  "destination_load_started",
  "table_load_started",
  "load_progress",
  "table_load_failed",
  "table_loaded",
  "load_completed",
  "schema_evolved",
  "resource_failed",
  "pipeline_error",
  "cancelled",
  "task_failed"
]);

// ── Service ────────────────────────────────────────────────────────────────

export class AirwayService {
  private static base(projectId: string): string {
    return `/${projectId}/agentic-airway`;
  }

  static async startRun(
    projectId: string,
    request: StartAirwayRequest
  ): Promise<{ run_id: string }> {
    const { data } = await apiClient.post(`${AirwayService.base(projectId)}/runs`, request);
    return data;
  }

  static async cancelRun(projectId: string, runId: string): Promise<void> {
    await apiClient.post(`${AirwayService.base(projectId)}/runs/${runId}/cancel`);
  }

  static async listRuns(
    projectId: string,
    pipelineRef: string,
    limit = 50
  ): Promise<AirwayRunSummary[]> {
    const { data } = await apiClient.get(`${AirwayService.base(projectId)}/runs`, {
      params: { pipeline_ref: pipelineRef, limit }
    });
    return data;
  }

  static async listFiles(projectId: string): Promise<AirwayFile[]> {
    const { data } = await apiClient.get(`${AirwayService.base(projectId)}/files`);
    return data;
  }

  /**
   * Stream events for a run. `onEvent` fires per known event type;
   * resolves when the stream closes (terminal) or `signal` aborts.
   */
  static async streamEvents(
    projectId: string,
    runId: string,
    options: {
      onEvent: (event: AirwayEvent) => void;
      onOpen?: () => void;
      onClose?: () => void;
      onError?: (error: Error) => void;
      signal?: AbortSignal;
    }
  ): Promise<void> {
    const url = `${apiBaseURL}${AirwayService.base(projectId)}/runs/${runId}/events`;
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
        if (!ev.event || !KNOWN_EVENTS.has(ev.event as AirwayEventType)) return;
        try {
          const payload = JSON.parse(ev.data);
          options.onEvent({
            type: ev.event as AirwayEventType,
            payload
          } as AirwayEvent);
        } catch (error) {
          console.error("Error parsing airway SSE data:", error);
        }
      },
      onerror(err) {
        console.error("airway SSE error:", err);
        options.onError?.(err);
        throw err;
      },
      onclose() {
        options.onClose?.();
      }
    });
  }
}
