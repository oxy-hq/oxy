import { fetchEventSource } from "@microsoft/fetch-event-source";
import { useCallback, useEffect, useRef, useState } from "react";
import type { HumanInputQuestion, UiBlock } from "@/services/api/analytics";
import { AppBuilderService } from "@/services/api/appBuilder";

// ── SSE event types ───────────────────────────────────────────────────────────

export const APP_BUILDER_SSE_TYPES = [
  "step_start",
  "step_end",
  "step_summary_update",
  "text_delta",
  "thinking_start",
  "thinking_token",
  "thinking_end",
  "tool_call",
  "tool_result",
  "awaiting_input",
  "human_input_resolved",
  "done",
  "error",
  "fan_out_start",
  "sub_spec_start",
  "sub_spec_end",
  "fan_out_end",
  // App-builder domain events
  "task_plan_ready",
  "task_sql_resolved",
  "task_executed",
  "app_yaml_ready",
  "llm_usage"
] as const;

export type AppBuilderSseType = (typeof APP_BUILDER_SSE_TYPES)[number];

/** Derive a typed SSE event from a UiBlock by mapping event_type → type and payload → data. */
type BlockToSseEvent<B> = B extends { event_type: infer T; payload: infer D }
  ? { id: string; type: T; data: D }
  : never;

export type SseEvent = BlockToSseEvent<UiBlock>;

// ── Run state machine ─────────────────────────────────────────────────────────

export type AppBuilderRunState =
  | { tag: "idle" }
  | { tag: "running"; runId: string; events: SseEvent[] }
  | {
      tag: "suspended";
      runId: string;
      events: SseEvent[];
      questions: HumanInputQuestion[];
    }
  | { tag: "done"; runId: string; durationMs: number; events: SseEvent[] }
  | { tag: "failed"; runId: string; message: string; durationMs: number; events: SseEvent[] };

// ── Mappers ───────────────────────────────────────────────────────────────────

export function uiBlockToSseEvent(e: UiBlock): SseEvent {
  return { id: String(e.seq), type: e.event_type, data: e.payload } as SseEvent;
}

export function sseEventToUiBlock(ev: SseEvent): UiBlock {
  return {
    seq: Number(ev.id),
    event_type: ev.type,
    payload: ev.data
  } as UiBlock;
}

// ── Hook options ──────────────────────────────────────────────────────────────

export interface UseAppBuilderRunOptions {
  projectId: string;
}

export interface UseAppBuilderRunResult {
  state: AppBuilderRunState;
  /** Start a brand-new run (creates the DB record + opens SSE). */
  start: (agentId: string, request: string, threadId?: string) => void;
  /** Resume from SSE using a run that already exists (page reload). */
  reconnect: (runId: string) => void;
  /** Restore a terminal run's state directly from pre-loaded events (no SSE). */
  hydrate: (
    runId: string,
    status: "done" | "failed",
    events: SseEvent[],
    errorMessage?: string
  ) => void;
  /** Submit an answer to a suspended run. */
  answer: (text: string) => void;
  /** Retry a failed run from its last checkpoint. */
  retry: () => void;
  /** Cancel the active run server-side, then close the SSE connection. */
  stop: () => void;
  reset: () => void;
  isStarting: boolean;
  isAnswering: boolean;
}

export function useAppBuilderRun({ projectId }: UseAppBuilderRunOptions): UseAppBuilderRunResult {
  const [state, setState] = useState<AppBuilderRunState>({ tag: "idle" });
  const abortRef = useRef<AbortController | null>(null);
  const [isStarting, setIsStarting] = useState(false);
  const [isAnswering, setIsAnswering] = useState(false);

  useEffect(() => {
    return () => {
      abortRef.current?.abort();
    };
  }, []);

  const appendEvent = useCallback((ev: SseEvent) => {
    setState((prev) => {
      if (prev.tag === "running" || prev.tag === "suspended") {
        return { ...prev, events: [...prev.events, ev] };
      }
      return prev;
    });
  }, []);

  const openStream = useCallback(
    (runId: string, existingEvents: SseEvent[] = []) => {
      abortRef.current?.abort();
      const controller = new AbortController();
      abortRef.current = controller;

      const url = AppBuilderService.eventsUrl(projectId, runId);
      const token = localStorage.getItem("auth_token");

      setState({ tag: "running", runId, events: existingEvents });

      fetchEventSource(url, {
        method: "GET",
        headers: {
          Authorization: token ?? ""
        },
        openWhenHidden: true,
        signal: controller.signal,
        async onopen(res) {
          if (res.status !== 200) {
            throw new Error(`SSE connection failed: ${res.status}`);
          }
        },
        onmessage(ev) {
          if (!ev.event) return;
          let parsed: Record<string, unknown> = {};
          try {
            parsed = JSON.parse(ev.data ?? "{}");
          } catch {
            // ignore malformed events
          }
          const sseEvent = { id: ev.id ?? "", type: ev.event, data: parsed } as SseEvent;
          appendEvent(sseEvent);

          if (ev.event === "awaiting_input") {
            setState((prev) => {
              if (prev.tag !== "running") return prev;
              return {
                tag: "suspended",
                runId,
                events: prev.events,
                questions: (parsed.questions as HumanInputQuestion[]) ?? []
              };
            });
          } else if (ev.event === "human_input_resolved") {
            setState((prev) => {
              if (prev.tag !== "suspended") return prev;
              return { tag: "running", runId, events: prev.events };
            });
          } else if (ev.event === "done") {
            controller.abort();
            setState((prev) => {
              const evs =
                prev.tag === "running" || prev.tag === "suspended" ? prev.events : existingEvents;
              return {
                tag: "done",
                runId,
                durationMs: (parsed.duration_ms as number) ?? 0,
                events: evs
              };
            });
          } else if (ev.event === "error") {
            controller.abort();
            setState((prev) => {
              const evs =
                prev.tag === "running" || prev.tag === "suspended" ? prev.events : existingEvents;
              return {
                tag: "failed",
                runId,
                message: (parsed.message as string) ?? "Unknown error",
                durationMs: (parsed.duration_ms as number) ?? 0,
                events: evs
              };
            });
          }
        },
        onerror(err) {
          if (controller.signal.aborted) {
            throw err;
          }
          console.error("App-builder SSE error:", err);
          setState((prev) => {
            const evs =
              prev.tag === "running" || prev.tag === "suspended" ? prev.events : existingEvents;
            if (prev.tag === "running" || prev.tag === "suspended") {
              return {
                tag: "failed",
                runId,
                message: "SSE connection lost",
                durationMs: 0,
                events: evs
              };
            }
            return prev;
          });
          throw err;
        }
      }).catch(() => {
        // fetchEventSource rejects on abort() — expected
      });
    },
    [projectId, appendEvent]
  );

  const start = useCallback(
    (agentId: string, request: string, threadId?: string) => {
      setIsStarting(true);
      setState({ tag: "running", runId: "", events: [] });
      AppBuilderService.createRun(projectId, { agent_id: agentId, request, thread_id: threadId })
        .then(({ run_id }) => {
          openStream(run_id);
        })
        .catch((err: unknown) => {
          setState({
            tag: "failed",
            runId: "",
            message: err instanceof Error ? err.message : "Failed to start run",
            durationMs: 0,
            events: []
          });
        })
        .finally(() => setIsStarting(false));
    },
    [projectId, openStream]
  );

  const reconnect = useCallback(
    (runId: string) => {
      openStream(runId);
    },
    [openStream]
  );

  const answer = useCallback(
    (text: string) => {
      if (state.tag !== "suspended") return;
      const { runId, events } = state;
      setState({ tag: "running", runId, events });
      setIsAnswering(true);
      AppBuilderService.submitAnswer(projectId, runId, text)
        .catch((err: unknown) => {
          abortRef.current?.abort();
          setState({
            tag: "failed",
            runId,
            message: `answer rejected: ${err instanceof Error ? err.message : String(err)}`,
            durationMs: 0,
            events: []
          });
        })
        .finally(() => setIsAnswering(false));
    },
    [state, projectId]
  );

  const reset = useCallback(() => {
    abortRef.current?.abort();
    setState({ tag: "idle" });
  }, []);

  const stop = useCallback(() => {
    const runId = state.tag === "running" || state.tag === "suspended" ? state.runId : null;
    abortRef.current?.abort();
    setState({ tag: "idle" });
    if (runId) {
      AppBuilderService.cancelRun(projectId, runId).catch(() => {
        // Best-effort
      });
    }
  }, [state, projectId]);

  const hydrate = useCallback(
    (runId: string, status: "done" | "failed", events: SseEvent[], errorMessage?: string) => {
      abortRef.current?.abort();
      if (status === "done") {
        setState({ tag: "done", runId, durationMs: 0, events });
      } else {
        setState({
          tag: "failed",
          runId,
          message: errorMessage ?? "Run failed",
          durationMs: 0,
          events
        });
      }
    },
    []
  );

  const retry = useCallback(() => {
    if (state.tag !== "failed" || !state.runId) return;
    const { runId } = state;
    setIsStarting(true);
    AppBuilderService.retryRun(projectId, runId)
      .then(() => {
        // Reconnect SSE — replays surviving events + streams new retry events.
        openStream(runId);
      })
      .catch((err: unknown) => {
        setState({
          tag: "failed",
          runId,
          message: err instanceof Error ? err.message : "Failed to retry run",
          durationMs: 0,
          events: state.events
        });
      })
      .finally(() => setIsStarting(false));
  }, [state, projectId, openStream]);

  return { state, start, reconnect, hydrate, answer, retry, stop, reset, isStarting, isAnswering };
}
