/**
 * Hooks for the `/agentic-airway` HTTP surface.
 *
 * Airway runs are queue-driven: `useStartAirwayRun` seeds the run, the
 * runtime coordinator drives it, and `useAirwayRunStream` consumes the
 * shared SSE endpoint and folds the event buffer through
 * `reduceAirwayEvents` into the phase-bar + resource-grid view model.
 *
 * Mirrors `useAgenticWorkflows.ts`. Simpler: no snapshot endpoint, no
 * step DAG — the view is a pure function of the accumulated events,
 * and `reduceAirwayEvents` is idempotent over a prefix so we just
 * re-reduce the whole buffer on every tick.
 */

import {
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient
} from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import {
  type AirwayEvent,
  type AirwayRunSummary,
  AirwayService,
  type StartAirwayRequest
} from "@/services/api/airway";
import { type AirwayRunView, reduceAirwayEvents } from "@/utils/airwayReducer";

import queryKeys from "../queryKey";

const keys = queryKeys.airway;

// ── Mutations ──────────────────────────────────────────────────────────────

const useStartAirwayRun = (): UseMutationResult<{ run_id: string }, Error, StartAirwayRequest> => {
  const { project } = useCurrentProjectBranch();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (request: StartAirwayRequest) => AirwayService.startRun(project.id, request),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: keys.runsForPipeline(project.id, variables.pipeline_ref)
      });
    }
  });
};

const useCancelAirwayRun = (): UseMutationResult<void, Error, string> => {
  const { project } = useCurrentProjectBranch();
  return useMutation({
    mutationFn: (runId: string) => AirwayService.cancelRun(project.id, runId)
  });
};

/** Run-history dropdown source — newest run first, capped at `limit`. */
export const useAirwayRuns = (
  pipelineRef: string,
  limit = 50
): UseQueryResult<AirwayRunSummary[]> => {
  const { project } = useCurrentProjectBranch();
  return useQuery({
    queryKey: keys.runsForPipeline(project.id, pipelineRef),
    queryFn: () => AirwayService.listRuns(project.id, pipelineRef, limit),
    enabled: !!pipelineRef
  });
};

// ── Run stream ─────────────────────────────────────────────────────────────

export type AirwayRunStreamHandle = {
  /** Folded view model for the phase bar + resource grid. */
  view: AirwayRunView;
  /** Raw events in arrival order — backs the "Raw event trace" panel. */
  events: AirwayEvent[];
  /** True while the SSE connection is open. */
  streaming: boolean;
};

const IDLE_VIEW: AirwayRunView = reduceAirwayEvents([]);

/**
 * Subscribe to a run's SSE stream and expose the reduced view. Passing
 * `undefined` resets to the idle view (used before a run is launched).
 */
export const useAirwayRunStream = (runId: string | undefined): AirwayRunStreamHandle => {
  const { project } = useCurrentProjectBranch();
  const queryClient = useQueryClient();
  const [events, setEvents] = useState<AirwayEvent[]>([]);
  const [streaming, setStreaming] = useState(false);
  const abortRef = useRef<AbortController | null>(null);

  useEffect(() => {
    if (!runId) {
      setEvents([]);
      setStreaming(false);
      return;
    }
    const controller = new AbortController();
    abortRef.current = controller;
    setEvents([]);
    setStreaming(true);

    AirwayService.streamEvents(project.id, runId, {
      signal: controller.signal,
      onEvent: (event) => {
        setEvents((prev) => [...prev, event]);
      },
      onError: (err) => {
        if (controller.signal.aborted) return;
        // Surface as a synthetic terminal error event so the reducer
        // drives the view to `failed` without a special code path.
        setEvents((prev) => [
          ...prev,
          {
            type: "pipeline_error",
            payload: { pipeline_name: "", load_id: null, error: err.message }
          }
        ]);
      },
      onClose: () => {
        if (controller.signal.aborted) return;
        setStreaming(false);
        // The run-history list keys off `agentic_runs`; drop it so a
        // freshly-terminal run shows the right status on next read.
        queryClient.invalidateQueries({
          queryKey: [...keys.all, "runs-for-pipeline", project.id]
        });
      }
    }).catch(() => {
      // Connection-level failure already surfaced via `onError`; the
      // `.catch` only stops an unhandled rejection.
    });

    return () => {
      controller.abort();
      setStreaming(false);
    };
  }, [runId, project.id, queryClient]);

  const view = useMemo(
    () => (events.length === 0 ? IDLE_VIEW : reduceAirwayEvents(events)),
    [events]
  );

  return { view, events, streaming };
};

// ── Controller (launch + stream + stop) ────────────────────────────────────

/**
 * Ties the start mutation, the SSE stream, and cancel into one handle
 * for the run page. `launch` starts a run and begins streaming; `stop`
 * cancels it. The reduced `view` updates live.
 */
export const useAirwayRunController = () => {
  const start = useStartAirwayRun();
  const cancel = useCancelAirwayRun();
  const [runId, setRunId] = useState<string | undefined>(undefined);
  const stream = useAirwayRunStream(runId);

  const launch = useCallback(
    async (request: StartAirwayRequest) => {
      const { run_id } = await start.mutateAsync(request);
      setRunId(run_id);
      return run_id;
    },
    [start]
  );

  const stop = useCallback(async () => {
    if (!runId) return;
    await cancel.mutateAsync(runId);
  }, [cancel, runId]);

  return {
    runId,
    /** Adopt an existing run id (e.g. deep-link / reload). */
    setRunId,
    launch,
    stop,
    starting: start.isPending,
    stopping: cancel.isPending,
    ...stream
  };
};
