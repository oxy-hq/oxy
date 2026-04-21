import { fetchEventSource } from "@microsoft/fetch-event-source";
import { useCallback, useEffect, useRef, useState } from "react";
import type {
  ChartConfig,
  HumanInputQuestion,
  ThinkingMode,
  UiBlock
} from "@/services/api/analytics";
import { AnalyticsService } from "@/services/api/analytics";

/** A self-contained display block received from the analytics pipeline. */
export type AnalyticsDisplayBlock = {
  config: ChartConfig;
  columns: string[];
  rows: unknown[][];
};

/**
 * Detect whether an `awaiting_input` event represents a procedure delegation
 * (not a human input question). Delegation prompts start with "Executing step:"
 * which the workflow orchestrator uses for step delegation.
 */
export function isDelegationSuspension(questions: HumanInputQuestion[]): boolean {
  if (questions.length === 0) return false;
  const prompt = questions[0].prompt;
  return (
    prompt.startsWith("Executing step:") ||
    prompt.startsWith("Execute procedure") ||
    prompt.startsWith("Delegating to builder") ||
    prompt.startsWith("The analytics pipeline could not")
  );
}

// ── SSE event types ───────────────────────────────────────────────────────────

export const ANALYTICS_SSE_TYPES = [
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
  "input_resolved",
  "done",
  "error",
  "cancelled",
  "fan_out_start",
  "sub_spec_start",
  "sub_spec_end",
  "fan_out_end",
  // Domain events
  "schema_resolved",
  "triage_completed",
  "intent_clarified",
  "spec_resolved",
  "query_generated",
  "query_executed",
  "analytics_validation_failed",
  "chart_rendered",
  // Procedure lifecycle (emitted by OxyProcedureRunner)
  "procedure_started",
  "procedure_completed",
  // Procedure step progress (emitted by WorkflowEventBridge)
  "procedure_step_started",
  "procedure_step_completed",
  // LLM usage metrics
  "llm_usage",
  // Recovery marker (transparent — same attempt, continued stream)
  "recovery_resumed",
  // Delegation lifecycle (emitted by coordinator for agent/workflow delegation)
  "delegation_started",
  "delegation_completed"
] as const;

export type AnalyticsSseType = (typeof ANALYTICS_SSE_TYPES)[number];

/** Derive a typed SSE event from a UiBlock by mapping event_type → type and payload → data. */
type BlockToSseEvent<B> = B extends { event_type: infer T; payload: infer D }
  ? { id: string; type: T; data: D }
  : never;

/** Strict discriminated union of all SSE events, derived from UiBlock.
 *  Mirrors SseEvent on the wire but with typed `data` per event type. */
export type SseEvent = BlockToSseEvent<UiBlock>;

// ── Stream segment ────────────────────────────────────────────────────────────

export interface StreamSegment {
  id: string;
  kind: "thinking" | "output" | "tool";
  text: string;
  done: boolean;
  toolName?: string;
  toolInput?: string;
  toolOutput?: string;
  toolDurationMs?: number;
}

export function buildStreamSegments(events: SseEvent[]): StreamSegment[] {
  const segments: StreamSegment[] = [];
  let counter = 0;
  const nextId = () => `seg-${counter++}`;

  for (const ev of events) {
    switch (ev.type) {
      case "thinking_start":
        segments.push({ id: nextId(), kind: "thinking", text: "", done: false });
        break;
      case "thinking_token": {
        const last = segments.at(-1);
        if (last?.kind === "thinking" && !last.done) {
          last.text += ev.data.token ?? "";
        } else {
          segments.push({ id: nextId(), kind: "thinking", text: ev.data.token ?? "", done: false });
        }
        break;
      }
      case "thinking_end": {
        const last = segments.at(-1);
        if (last?.kind === "thinking") last.done = true;
        break;
      }
      case "text_delta": {
        const last = segments.at(-1);
        if (last?.kind === "output" && !last.done) {
          last.text += ev.data.token ?? "";
        } else {
          segments.push({ id: nextId(), kind: "output", text: ev.data.token ?? "", done: false });
        }
        break;
      }
      case "step_end": {
        const last = segments.at(-1);
        if (last?.kind === "output") last.done = true;
        break;
      }
      case "tool_call":
        segments.push({
          id: nextId(),
          kind: "tool",
          text: "",
          done: false,
          toolName: ev.data.name ?? "",
          toolInput: JSON.stringify(ev.data.input ?? "")
        });
        break;
      case "tool_result": {
        const pending =
          segments
            .slice()
            .reverse()
            .find((s) => s.kind === "tool" && !s.done && s.toolName === (ev.data.name ?? "")) ??
          segments
            .slice()
            .reverse()
            .find((s) => s.kind === "tool" && !s.done);
        if (pending) {
          pending.toolOutput = JSON.stringify(ev.data.output ?? "");
          pending.toolDurationMs = ev.data.duration_ms;
          pending.done = true;
        }
        break;
      }
    }
  }
  return segments;
}

export function extractAnswer(events: SseEvent[]): string {
  return buildStreamSegments(events)
    .filter((s) => s.kind === "output")
    .map((s) => s.text)
    .join("")
    .trim();
}

/** Extract all chart_rendered payloads from an event list, in emission order. */
export function extractDisplayBlocks(events: SseEvent[]): AnalyticsDisplayBlock[] {
  return events
    .filter((ev) => ev.type === "chart_rendered")
    .map((ev) => ev.data as AnalyticsDisplayBlock);
}

/**
 * Find the single chart_rendered block that corresponds to a specific render_chart
 * tool_call identified by its SSE sequence number. Returns null if not found.
 */
export function extractDisplayBlockForSeq(
  events: SseEvent[],
  toolCallSeq: number
): AnalyticsDisplayBlock | null {
  const toolCallIdx = events.findIndex(
    (ev) => ev.type === "tool_call" && ev.id === String(toolCallSeq)
  );
  if (toolCallIdx === -1) return null;
  for (let i = toolCallIdx + 1; i < events.length; i++) {
    if (events[i].type === "chart_rendered") {
      return events[i].data as AnalyticsDisplayBlock;
    }
  }
  return null;
}

// ── Run state machine ─────────────────────────────────────────────────────────

export type AnalyticsRunState =
  | { tag: "idle" }
  | { tag: "running"; runId: string; events: SseEvent[] }
  | {
      tag: "suspended";
      runId: string;
      events: SseEvent[];
      questions: HumanInputQuestion[];
    }
  | {
      tag: "done";
      runId: string;
      answer: string;
      displayBlocks: AnalyticsDisplayBlock[];
      durationMs: number;
      events: SseEvent[];
    }
  | { tag: "failed"; runId: string; message: string; durationMs: number; events: SseEvent[] }
  | { tag: "cancelled"; runId: string; events: SseEvent[] };

// ── Mappers ───────────────────────────────────────────────────────────────────

/** Convert a REST `UiBlock` (from list_runs_by_thread) to the `SseEvent` shape
 *  used everywhere else in the UI. */
export function uiBlockToSseEvent(e: UiBlock): SseEvent {
  // Cast is safe: event_type and payload are structurally identical to type and data.
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

export interface UseAnalyticsRunOptions {
  projectId: string;
}

export interface UseAnalyticsRunResult {
  state: AnalyticsRunState;
  /** Start a brand-new run (creates the DB record + opens SSE). */
  start: (
    agentId: string,
    question: string,
    threadId?: string,
    thinkingMode?: ThinkingMode,
    model?: string
  ) => void;
  /** Resume from SSE using a run that already exists (page reload). */
  reconnect: (runId: string, existingState?: AnalyticsRunState["tag"]) => void;
  /** Restore a terminal run's state directly from pre-loaded events (no SSE). */
  hydrate: (
    runId: string,
    status: "done" | "failed",
    events: SseEvent[],
    errorMessage?: string
  ) => void;
  /** Submit an answer to a suspended run. */
  answer: (text: string) => void;
  /** Cancel the active run server-side, then close the SSE connection. */
  stop: () => void;
  reset: () => void;
  isStarting: boolean;
  isAnswering: boolean;
}

export function useAnalyticsRun({ projectId }: UseAnalyticsRunOptions): UseAnalyticsRunResult {
  const [state, setState] = useState<AnalyticsRunState>({ tag: "idle" });
  const abortRef = useRef<AbortController | null>(null);
  const [isStarting, setIsStarting] = useState(false);
  const [isAnswering, setIsAnswering] = useState(false);

  // Abort the SSE stream when the hook unmounts (e.g. component navigates away).
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

      const url = AnalyticsService.eventsUrl(projectId, runId);
      const token = localStorage.getItem("auth_token");

      // Compute the last seen event seq so the SSE endpoint skips
      // already-delivered events (critical for cold resume reconnects).
      const lastSeq =
        existingEvents.length > 0 ? existingEvents[existingEvents.length - 1].id : undefined;

      setState({ tag: "running", runId, events: existingEvents });

      fetchEventSource(url, {
        method: "GET",
        headers: {
          Authorization: token ?? "",
          ...(lastSeq != null && { "Last-Event-ID": lastSeq })
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
          // Cast is safe: ev.event and parsed come from the SSE wire at this boundary.
          const sseEvent = { id: ev.id ?? "", type: ev.event, data: parsed } as SseEvent;
          appendEvent(sseEvent);

          if (ev.event === "recovery_resumed") {
            // Transparent recovery marker — keep all events, stay in running state.
            return;
          }
          if (ev.event === "awaiting_input") {
            const questions = (parsed.questions as HumanInputQuestion[]) ?? [];
            // Don't show suspension popup for procedure delegations —
            // the procedure progress renders inline via ProcedureChild/ProcedureDelegationCard.
            if (isDelegationSuspension(questions)) {
              // Stay in "running" state — procedure events will render inline.
              return;
            }
            setState((prev) => {
              if (prev.tag !== "running") return prev;
              return {
                tag: "suspended",
                runId,
                events: prev.events,
                questions
              };
            });
          } else if (ev.event === "input_resolved") {
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
                answer: extractAnswer(evs),
                displayBlocks: extractDisplayBlocks(evs),
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
          } else if (ev.event === "cancelled") {
            controller.abort();
            setState((prev) => {
              const evs =
                prev.tag === "running" || prev.tag === "suspended" ? prev.events : existingEvents;
              return { tag: "cancelled", runId, events: evs };
            });
          }
        },
        onerror(err) {
          // Intentional aborts (thread switch, stop, unmount) should not surface as errors.
          if (controller.signal.aborted) {
            throw err;
          }
          console.error("Analytics SSE error:", err);
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
          throw err; // stop retrying
        }
      }).catch(() => {
        // fetchEventSource rejects when abort() is called — that's expected
      });
    },
    [projectId, appendEvent]
  );

  const start = useCallback(
    (
      agentId: string,
      question: string,
      threadId?: string,
      thinkingMode?: ThinkingMode,
      model?: string
    ) => {
      setIsStarting(true);
      setState({ tag: "running", runId: "", events: [] });
      AnalyticsService.createRun(projectId, {
        agent_id: agentId,
        question,
        thread_id: threadId,
        thinking_mode: thinkingMode,
        ...(agentId === "__builder__" && { domain: "builder", model })
      })
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
    (runId: string, _existingTag?: AnalyticsRunState["tag"]) => {
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
      AnalyticsService.submitAnswer(projectId, runId, text)
        .then((res) => {
          if (res.resumed) {
            // Cold resume: pipeline was rebuilt server-side after a restart.
            // Reconnect SSE so we receive events from the new pipeline.
            openStream(runId, events);
          }
        })
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
    [state, projectId, openStream]
  );

  const reset = useCallback(() => {
    abortRef.current?.abort();
    setState({ tag: "idle" });
  }, []);

  const stop = useCallback(() => {
    if (state.tag !== "running" && state.tag !== "suspended") return;
    const { runId, events } = state;
    abortRef.current?.abort();
    setState({ tag: "failed", runId, message: "Cancelled", durationMs: 0, events });
    AnalyticsService.cancelRun(projectId, runId).catch(() => {
      // Best-effort — SSE is already closed; server will clean up on reconnect
    });
  }, [state, projectId]);

  const hydrate = useCallback(
    (runId: string, status: "done" | "failed", events: SseEvent[], errorMessage?: string) => {
      abortRef.current?.abort();
      if (status === "done") {
        setState({
          tag: "done",
          runId,
          answer: extractAnswer(events),
          displayBlocks: extractDisplayBlocks(events),
          durationMs: 0,
          events
        });
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

  return { state, start, reconnect, hydrate, answer, stop, reset, isStarting, isAnswering };
}
