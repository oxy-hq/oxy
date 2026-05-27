import type { RunEventEntry } from "@/services/api/coordinator";

/**
 * Waterfall model. Pure derivation from the persisted event log — no
 * server-side schema. Times are millisecond offsets from the run's
 * first event; durations span the wall-clock window of the matching
 * start/end pair.
 *
 * Phases come from `state_enter` / `state_exit` pairs (one per FSM
 * state visit; multiple visits to the same state — e.g. Solving
 * re-entered after Diagnosing — produce separate phase entries).
 * Child spans are the LLM rounds, thinking blocks, and tool calls
 * that happened *within* that phase's time window.
 */
export interface WaterfallModel {
  /** Total wall-clock duration of the run in ms. */
  totalMs: number;
  /** Sequential phase visits, in occurrence order. */
  phases: PhaseSpan[];
  /** Unmatched LLM/tool/thinking spans whose phase couldn't be
   *  resolved (e.g. events before the first state_enter, or after a
   *  state_exit). Rare in practice; surfaced for debugging. */
  orphans: ChildSpan[];
}

export interface PhaseSpan {
  /** FSM state name, e.g. "clarifying". */
  state: string;
  /** Sequential index across all phase entries — used as a stable
   *  React key when the same state is visited twice. */
  index: number;
  startMs: number;
  endMs: number;
  durationMs: number;
  children: ChildSpan[];
  /** Per-phase rollups for the right-aligned stats column. */
  llmCalls: number;
  toolCalls: number;
  totalTokens: number;
}

export type ChildSpanKind = "llm" | "thinking" | "tool" | "subrun" | "query" | "step";

export interface ChildSpan {
  kind: ChildSpanKind;
  /** Stable id derived from start event seq — used as React key. */
  id: string;
  /** Display label: model name for LLM, tool name for tool, "thinking · <state>" for thinking,
   *  "↪ <target>" for a delegated sub-run. */
  label: string;
  startMs: number;
  endMs: number;
  durationMs: number;
  /** For LLM spans only. */
  llm?: {
    model: string;
    promptTokens: number;
    outputTokens: number;
    cacheCreationTokens: number;
    cacheReadTokens: number;
  };
  /** For tool spans only — raw input/output for the hover preview. */
  tool?: {
    name: string;
    input: unknown;
    output: unknown;
    error?: string;
  };
  /** For thinking spans — the FSM state the model was reasoning in. */
  thinking?: {
    state: string;
  };
  /** For sub-run (delegation) spans — the child task's target + the
   *  request the parent sent + final answer/error. */
  subrun?: {
    target: string;
    request: string;
    success: boolean;
    answer?: string;
    error?: string;
    /** Nested events from the child run — surfaced as a mini-waterfall
     *  in the side panel. */
    nested: WaterfallModel;
  };
  /** For SQL/query execution spans — what actually ran during the
   *  Executing phase. The Executing phase doesn't make any LLM calls,
   *  so without this child the phase would render as a duration-only
   *  bar with nothing to inspect. */
  query?: {
    sql: string;
    rowCount: number;
    success: boolean;
    error?: string;
    /** "Semantic" / "Llm" / "Vendor" / "Preagg" — drives the verified-query badge. */
    source: string;
    isPreagg: boolean;
    columns: string[];
    /** First few result rows (capped server-side). */
    rowsPreview: unknown[][];
  };
  /** For procedure-step spans inside a delegated procedure run. */
  step?: {
    name: string;
    success: boolean;
    error?: string;
  };
  /** "ok" for normal completion, "error" for tool errors / unmatched ends. */
  status: "ok" | "error" | "running";
}

interface OpenLlm {
  startSeq: number;
  startTs: number;
  state: string;
  promptTokens: number;
}

interface OpenThinking {
  startSeq: number;
  startTs: number;
  state: string;
}

interface OpenTool {
  startSeq: number;
  startTs: number;
  name: string;
  input: unknown;
}

interface OpenPhase {
  startSeq: number;
  startTs: number;
  state: string;
}

interface OpenStep {
  startSeq: number;
  startTs: number;
  name: string;
}

const num = (v: unknown): number => (typeof v === "number" ? v : 0);
const str = (v: unknown): string => (typeof v === "string" ? v : "");

/**
 * Reconstruct a waterfall from a run's filtered event log.
 *
 * Strategy: walk events in seq order, maintain a stack of open
 * intervals (phase / llm / thinking / tool), close them on the
 * matching `*_end` event, and assign closed child spans to whichever
 * phase was open at their start time. Time positioning uses the
 * `duration_ms` field on `_end` events as ground truth — these are
 * recorded with real wall-clock measurements at write time. Phase
 * start times are derived by laying child durations back-to-back
 * inside each phase window, which gives a recognisably accurate
 * waterfall without needing a per-event timestamp column.
 */
export const buildWaterfall = (events: RunEventEntry[]): WaterfallModel => {
  if (events.length === 0) {
    return { totalMs: 0, phases: [], orphans: [] };
  }

  // Cursor in ms — monotonic, advances by each completed event's
  // duration_ms. Events that don't carry duration (start markers,
  // validation flags) don't advance the cursor.
  let cursor = 0;
  let phaseIdx = 0;

  const openLlms: OpenLlm[] = [];
  const openThinking: OpenThinking[] = [];
  const openTools: OpenTool[] = [];
  // Open procedure steps, keyed by step name. Subrun_step_* events pair
  // by name (no trace id), so concurrent steps with duplicate names
  // would collide — fine in practice; v1 procedures are linear.
  const openSteps = new Map<string, OpenStep>();
  let openPhase: OpenPhase | null = null;
  let phaseChildren: ChildSpan[] = [];
  let phaseLlmCalls = 0;
  let phaseToolCalls = 0;
  let phaseTokens = 0;

  const phases: PhaseSpan[] = [];
  const orphans: ChildSpan[] = [];

  // Delegation bookkeeping. Each `delegation_started` opens a slot;
  // `delegation_event` rows pile inner events into that slot;
  // `delegation_completed` closes it and emits a single sub-run child
  // span carrying a recursively-built mini-waterfall. Keyed by
  // child_task_id so concurrent delegations don't cross-pollinate.
  type OpenSubrun = {
    startSeq: number;
    startTs: number;
    target: string;
    request: string;
    innerEvents: RunEventEntry[];
  };
  const openSubruns = new Map<string, OpenSubrun>();

  const closePhase = (endTs: number) => {
    if (!openPhase) return;
    phases.push({
      state: openPhase.state,
      index: phaseIdx++,
      startMs: openPhase.startTs,
      endMs: endTs,
      durationMs: Math.max(endTs - openPhase.startTs, 0),
      children: phaseChildren,
      llmCalls: phaseLlmCalls,
      toolCalls: phaseToolCalls,
      totalTokens: phaseTokens
    });
    openPhase = null;
    phaseChildren = [];
    phaseLlmCalls = 0;
    phaseToolCalls = 0;
    phaseTokens = 0;
  };

  const pushChild = (span: ChildSpan) => {
    if (openPhase) {
      phaseChildren.push(span);
      if (span.kind === "llm") {
        phaseLlmCalls += 1;
        if (span.llm) {
          phaseTokens +=
            span.llm.promptTokens +
            span.llm.outputTokens +
            span.llm.cacheCreationTokens +
            span.llm.cacheReadTokens;
        }
      } else if (span.kind === "tool") {
        phaseToolCalls += 1;
      } else if (span.kind === "subrun" && span.subrun) {
        // Roll the sub-run's nested LLM / tool work into the parent
        // phase's stats so a delegating phase doesn't read as "0 LLM
        // calls" when in reality its child agent ran twenty.
        for (const np of span.subrun.nested.phases) {
          phaseLlmCalls += np.llmCalls;
          phaseToolCalls += np.toolCalls;
          phaseTokens += np.totalTokens;
        }
      }
    } else {
      orphans.push(span);
    }
  };

  for (const e of events) {
    const p = e.payload as Record<string, unknown>;
    switch (e.event_type) {
      case "state_enter": {
        // Auto-close a stale phase if state_exit was dropped.
        if (openPhase) closePhase(cursor);
        openPhase = { startSeq: e.seq, startTs: cursor, state: str(p.state) };
        break;
      }
      case "state_exit": {
        closePhase(cursor);
        break;
      }
      case "back_edge": {
        // Treat as a phase boundary even when the surrounding state_enter/exit
        // pair is also present — back-edges almost always coincide with a state
        // change, so this is a no-op in well-formed logs.
        break;
      }
      case "llm_start": {
        openLlms.push({
          startSeq: e.seq,
          startTs: cursor,
          state: str(p.state),
          promptTokens: num(p.prompt_tokens)
        });
        break;
      }
      case "llm_end": {
        const open = openLlms.pop();
        const duration = num(p.duration_ms);
        const endTs = cursor + duration;
        const startTs = open?.startTs ?? cursor;
        pushChild({
          kind: "llm",
          id: `llm-${open?.startSeq ?? e.seq}`,
          label: str(p.model) || "llm",
          startMs: startTs,
          endMs: endTs,
          durationMs: duration,
          llm: {
            model: str(p.model),
            promptTokens: open?.promptTokens ?? 0,
            outputTokens: num(p.output_tokens),
            cacheCreationTokens: num(p.cache_creation_input_tokens),
            cacheReadTokens: num(p.cache_read_input_tokens)
          },
          status: "ok"
        });
        cursor = endTs;
        break;
      }
      case "thinking_start": {
        openThinking.push({ startSeq: e.seq, startTs: cursor, state: str(p.state) });
        break;
      }
      case "thinking_end": {
        const open = openThinking.pop();
        // Thinking events don't carry duration_ms; treat as a marker
        // span whose duration is the gap between start and end on the
        // monotonic cursor (typically zero unless tool/LLM events
        // happened between them).
        if (open) {
          pushChild({
            kind: "thinking",
            id: `think-${open.startSeq}`,
            label: open.state ? `thinking · ${open.state}` : "thinking",
            startMs: open.startTs,
            endMs: cursor,
            durationMs: Math.max(cursor - open.startTs, 0),
            thinking: { state: open.state },
            status: "ok"
          });
        }
        break;
      }
      case "tool_call": {
        openTools.push({
          startSeq: e.seq,
          startTs: cursor,
          name: str(p.name) || "tool",
          input: p.input ?? null
        });
        break;
      }
      case "tool_result": {
        // ToolResult is matched to the most recent open tool of any
        // name — providers don't always pair by trace id. If multiple
        // tools are concurrent (rare in v1) the latest opens first.
        const open = openTools.pop();
        const duration = num(p.duration_ms);
        const endTs = cursor + duration;
        const isError = Boolean(p.error) || Boolean(p.is_error);
        if (open) {
          pushChild({
            kind: "tool",
            id: `tool-${open.startSeq}`,
            label: open.name,
            startMs: open.startTs,
            endMs: endTs,
            durationMs: duration,
            tool: {
              name: open.name,
              input: open.input,
              output: p.output ?? p.result ?? null,
              error: typeof p.error === "string" ? p.error : undefined
            },
            status: isError ? "error" : "ok"
          });
        }
        cursor = endTs;
        break;
      }
      case "delegation_started": {
        const id = str(p.child_task_id);
        if (!id) break;
        openSubruns.set(id, {
          startSeq: e.seq,
          startTs: cursor,
          target: str(p.target) || "delegated",
          request: str(p.request),
          innerEvents: []
        });
        break;
      }
      case "delegation_event": {
        const id = str(p.child_task_id);
        const open = openSubruns.get(id);
        if (!open) break;
        // Stash the inner event as if it were a child-run-native event;
        // we'll feed the array to buildWaterfall recursively when the
        // delegation closes.
        open.innerEvents.push({
          seq: e.seq,
          event_type: str(p.inner_event_type),
          payload: (p.inner_payload as Record<string, unknown>) ?? {}
        });
        break;
      }
      case "delegation_completed": {
        const id = str(p.child_task_id);
        const open = openSubruns.get(id);
        if (!open) break;
        openSubruns.delete(id);
        const nested = buildWaterfall(open.innerEvents);
        // Use the nested run's measured total as the sub-run duration —
        // delegation_event envelopes carry the inner *_end events with
        // their real duration_ms, so the recursive build gives us a
        // reliable wall-clock figure.
        const duration = nested.totalMs;
        const endTs = open.startTs + duration;
        const success = Boolean(p.success);
        pushChild({
          kind: "subrun",
          id: `subrun-${open.startSeq}`,
          label: `↪ ${open.target}`,
          startMs: open.startTs,
          endMs: endTs,
          durationMs: duration,
          subrun: {
            target: open.target,
            request: open.request,
            success,
            answer: typeof p.answer === "string" ? p.answer : undefined,
            error: typeof p.error === "string" ? p.error : undefined,
            nested
          },
          status: success ? "ok" : "error"
        });
        cursor = endTs;
        break;
      }
      // SQL execution — the *what* of the Executing phase. We don't
      // model query_generated as its own span (it'd be a zero-duration
      // marker in Solving); the SQL just rides along on the matching
      // query_executed below.
      case "query_executed": {
        const duration = num(p.duration_ms);
        const success = Boolean(p.success);
        const startTs = cursor;
        const endTs = cursor + duration;
        const sourceObj = p.source;
        const source =
          typeof sourceObj === "string"
            ? sourceObj
            : sourceObj && typeof sourceObj === "object"
              ? (Object.keys(sourceObj as Record<string, unknown>)[0] ?? "Llm")
              : "Llm";
        const sql = str(p.query);
        const labelHint = sql ? compactSqlLabel(sql) : "query";
        pushChild({
          kind: "query",
          id: `query-${e.seq}`,
          label: labelHint,
          startMs: startTs,
          endMs: endTs,
          durationMs: duration,
          query: {
            sql,
            rowCount: num(p.row_count),
            success,
            error: typeof p.error === "string" ? p.error : undefined,
            source,
            isPreagg: Boolean(p.is_preagg),
            columns: Array.isArray(p.columns)
              ? (p.columns as unknown[]).filter((c): c is string => typeof c === "string")
              : [],
            rowsPreview: Array.isArray(p.rows) ? (p.rows as unknown[][]) : []
          },
          status: success ? "ok" : "error"
        });
        cursor = endTs;
        break;
      }
      case "execution_failed": {
        // Emitted when the Executing stage errors *without* a paired
        // query_executed (e.g. dry_run rejected before submission, or
        // connector unreachable). Surfaces an error-tone bar so the
        // user can read the failure in-context.
        const sql = str(p.query);
        pushChild({
          kind: "query",
          id: `query-fail-${e.seq}`,
          label: sql ? compactSqlLabel(sql) : "execution failed",
          startMs: cursor,
          endMs: cursor,
          durationMs: 0,
          query: {
            sql,
            rowCount: 0,
            success: false,
            error: str(p.error) || undefined,
            source: str(p.source) || "Llm",
            isPreagg: false,
            columns: [],
            rowsPreview: []
          },
          status: "error"
        });
        break;
      }
      // Procedure-step lifecycle inside a delegated subrun. Step
      // events don't carry a duration; we measure wall-clock between
      // start/end on the monotonic cursor instead.
      case "subrun_step_started": {
        openSteps.set(str(p.step), { startSeq: e.seq, startTs: cursor, name: str(p.step) });
        break;
      }
      case "subrun_step_completed": {
        const name = str(p.step);
        const open = openSteps.get(name);
        if (!open) break;
        openSteps.delete(name);
        const success = Boolean(p.success);
        pushChild({
          kind: "step",
          id: `step-${open.startSeq}`,
          label: name,
          startMs: open.startTs,
          endMs: cursor,
          durationMs: Math.max(cursor - open.startTs, 0),
          step: {
            name,
            success,
            error: typeof p.error === "string" ? p.error : undefined
          },
          status: success ? "ok" : "error"
        });
        break;
      }
      default:
        break;
    }
  }

  // Close any straggling phase at the end of the log.
  if (openPhase) closePhase(cursor);

  return {
    totalMs: cursor,
    phases,
    orphans
  };
};

/** FSM phase → sequential primary-tint ramp through the happy path,
 *  with `diagnosing` (the back-edge / recovery state) called out in
 *  destructive tone since it represents a retry rather than forward
 *  progress.
 *
 *  Previously each phase had its own hue (sky / indigo / violet /
 *  emerald / amber / rose) — six categorical colors for steps the
 *  reader already encounters in *order*. That looked Crayola-busy and
 *  competed with status colors (emerald already means "done"). A
 *  single-hue ramp keeps progression legible without rainbow-encoding
 *  ordinal information. */
export const PHASE_COLORS: Record<string, { bg: string; border: string; text: string }> = {
  clarifying: { bg: "bg-primary/30", border: "border-primary/40", text: "text-foreground" },
  specifying: { bg: "bg-primary/45", border: "border-primary/55", text: "text-foreground" },
  solving: { bg: "bg-primary/60", border: "border-primary/70", text: "text-foreground" },
  executing: { bg: "bg-primary/75", border: "border-primary/85", text: "text-foreground" },
  interpreting: { bg: "bg-primary/85", border: "border-primary", text: "text-foreground" },
  diagnosing: {
    bg: "bg-destructive/50",
    border: "border-destructive/60",
    text: "text-destructive"
  }
};

export const colorsFor = (state: string) =>
  PHASE_COLORS[state.toLowerCase()] ?? {
    bg: "bg-muted-foreground/40",
    border: "border-border",
    text: "text-muted-foreground"
  };

/** Pull a short label out of a SQL string for the bar row — the first
 *  table mentioned after FROM, or the first 32 chars of the verb line.
 *  Keeps the executing-phase bar self-describing without exposing the
 *  full query (that lives in the side panel). */
export const compactSqlLabel = (sql: string): string => {
  const flat = sql.replace(/\s+/g, " ").trim();
  const fromMatch = flat.match(/\bFROM\s+([a-zA-Z0-9_."]+)/i);
  if (fromMatch) return `SELECT … FROM ${fromMatch[1]}`;
  return flat.slice(0, 40) + (flat.length > 40 ? "…" : "");
};

/** Compact ms formatter: "240ms" / "1.4s" / "12.4s". */
export const formatMs = (ms: number): string => {
  if (ms < 1000) return `${Math.round(ms)}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(ms < 10_000 ? 1 : 0)}s`;
  const mins = Math.floor(ms / 60_000);
  const secs = Math.round((ms % 60_000) / 1000);
  return `${mins}m ${secs}s`;
};
