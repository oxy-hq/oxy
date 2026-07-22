// Reasoning-trace builder for the Ask dock: folds the raw agent-run SSE
// events (`useAgentRun().events`) into the same step/tool/SQL shape the
// main web-app's AnalyticsReasoningTrace renders — the bundle stream goes
// through the same server-side EventRegistry processor, so the event
// vocabulary matches (`step_start`/`step_end`, `tool_call`/`tool_result`,
// `query_executed`, …). Deliberately lighter than the web-app hook: no
// fan-out cards, no thinking token stream, no artifact sidebar — labels,
// statuses, and result summaries only.

import type { AgentRunEvent } from "../customer-app/react";

export type TraceStepStatus = "running" | "done" | "failed" | "suspended";

export interface TraceItem {
  id: string;
  kind: "tool" | "sql" | "note" | "thinking";
  label: string;
  /** Muted inline preview of the tool input / query intent. */
  preview?: string;
  /** Right-aligned detail, e.g. "22 rows · 1.4s". */
  detail?: string;
  /** Body text (thinking items) — rendered as a wrapped paragraph. */
  text?: string;
  /** Full tool input / SQL, pretty-printed — the expandable detail body. */
  input?: string;
  /** Full tool output / result summary, pretty-printed. */
  output?: string;
  error?: boolean;
  running?: boolean;
}

export interface TraceStep {
  id: string;
  label: string;
  summary?: string;
  status: TraceStepStatus;
  items: TraceItem[];
}

const str = (v: unknown): string | undefined => (typeof v === "string" ? v : undefined);
const num = (v: unknown): number | undefined => (typeof v === "number" ? v : undefined);

/** "search_catalog" -> "Search Catalog" — matches the web-app pill labels. */
export function prettyToolName(name: string): string {
  return name
    .split(/[_-]+/)
    .filter(Boolean)
    .map((w) => w[0].toUpperCase() + w.slice(1))
    .join(" ");
}

/** Compact one-line preview of a tool input (string arg or first string
 *  field of an object), truncated for the trace row. */
function inputPreview(input: unknown, max = 60): string | undefined {
  // The stream often pre-serializes the tool input — unwrap JSON strings so
  // the preview shows the human part ("top stores by revenue"), not braces.
  if (typeof input === "string" && /^[[{]/.test(input.trim())) {
    try {
      return inputPreview(JSON.parse(input), max);
    } catch {
      // fall through — treat as plain text
    }
  }
  let text: string | undefined;
  if (typeof input === "string") text = input;
  else if (Array.isArray(input)) {
    const first = input.find((v) => typeof v === "string" && v.trim());
    text = typeof first === "string" ? first : JSON.stringify(input);
  } else if (input && typeof input === "object") {
    for (const v of Object.values(input as Record<string, unknown>)) {
      if (typeof v === "string" && v.trim()) {
        text = v;
        break;
      }
    }
    if (!text) {
      const json = JSON.stringify(input);
      text = json === "{}" ? undefined : json;
    }
  }
  if (!text) return undefined;
  text = text.replace(/\s+/g, " ").trim();
  return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}

/** Full pretty-printed payload for the expandable row detail: unwraps
 *  pre-serialized JSON strings, indents objects, caps runaway sizes. */
function prettyPayload(v: unknown, max = 2000): string | undefined {
  if (v === null || v === undefined) return undefined;
  let val: unknown = v;
  if (typeof val === "string") {
    const t = val.trim();
    if (!t) return undefined;
    if (/^[[{]/.test(t)) {
      try {
        val = JSON.parse(t);
      } catch {
        // keep the raw string
      }
    }
  }
  const text = typeof val === "string" ? val : JSON.stringify(val, null, 2);
  if (!text || text === "{}" || text === "[]") return undefined;
  return text.length > max ? `${text.slice(0, max)}\n…` : text;
}

export function formatMs(ms: number): string {
  return ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${ms}ms`;
}

/** Header meta, same as the web-app trace: LLM call count + total LLM time
 *  folded from `llm_usage` events. */
export function aggregateLlmStats(events: AgentRunEvent[]): { calls: number; totalMs: number } {
  let calls = 0;
  let totalMs = 0;
  for (const ev of events) {
    if (ev.type !== "llm_usage") continue;
    calls++;
    const d = (ev.data ?? {}) as Record<string, unknown>;
    totalMs += num(d.duration_ms) ?? 0;
  }
  return { calls, totalMs };
}

/** Reconstruct the streamed answer markdown from persisted events —
 *  concatenate the `text_delta` tokens, the same accumulation the live
 *  `useAgentRun` does. Used to rebuild a turn's answer on history restore. */
export function extractAnswer(events: AgentRunEvent[]): string {
  let out = "";
  for (const ev of events) {
    if (ev.type !== "text_delta") continue;
    const d = (ev.data ?? {}) as Record<string, unknown>;
    if (typeof d.token === "string") out += d.token;
  }
  return out;
}

/** Pull the clarifying-question prompt from a persisted `awaiting_input`
 *  event (`{ questions: [{ prompt, suggestions }] }`) so a turn that ended
 *  in a suspension restores with its question instead of a blank answer.
 *  Mirrors the live `useAgentRun` clarification parse. */
export function extractClarification(events: AgentRunEvent[]): string | null {
  for (const ev of events) {
    if (ev.type !== "awaiting_input") continue;
    const d = (ev.data ?? {}) as Record<string, unknown>;
    const questions = Array.isArray(d.questions) ? d.questions : [];
    const first = questions[0] as Record<string, unknown> | undefined;
    if (first && typeof first.prompt === "string") return first.prompt;
    // Older single-question shape: `{ question: "…" }`.
    if (typeof d.question === "string") return d.question;
  }
  return null;
}

function sqlDetail(d: Record<string, unknown>): string | undefined {
  const parts: string[] = [];
  const rows = num(d.row_count) ?? (Array.isArray(d.rows) ? d.rows.length : undefined);
  if (rows !== undefined) parts.push(`${rows} row${rows === 1 ? "" : "s"}`);
  const ms = num(d.duration_ms);
  if (ms !== undefined) parts.push(formatMs(ms));
  return parts.length ? parts.join(" · ") : undefined;
}

/** Mark any still-running items in a step as settled — called when the step
 *  closes so a tool/SQL/semantic row whose matching result never arrived
 *  (cancel, error mid-flight, terminal event) doesn't spin forever on a
 *  finished turn. */
function settleStepItems(step: TraceStep, asError = false): void {
  for (const item of step.items) {
    if (item.running) {
      item.running = false;
      if (asError) item.error = true;
    }
  }
}

/** Fold raw SSE events into display steps. Pure — safe to re-run on every
 *  render over the accumulated event array. */
export function buildTraceSteps(events: AgentRunEvent[]): TraceStep[] {
  const steps: TraceStep[] = [];
  let open: TraceStep | null = null;
  let n = 0;
  const id = (prefix: string) => `${prefix}-${n++}`;

  const ensureStep = (): TraceStep => {
    if (open) return open;
    open = { id: id("step"), label: "Working", status: "running", items: [] };
    steps.push(open);
    return open;
  };
  const lastRunning = (kind: TraceItem["kind"]): TraceItem | undefined => {
    for (let i = steps.length - 1; i >= 0; i--) {
      const items = steps[i].items;
      for (let j = items.length - 1; j >= 0; j--) {
        const item = items[j];
        if (item.kind === kind && item.running) return item;
      }
    }
    return undefined;
  };

  for (const ev of events) {
    const d = (ev.data ?? {}) as Record<string, unknown>;
    switch (ev.type) {
      case "step_start": {
        // A dangling open step means its step_end never arrived (crash /
        // recovery) — close it as done rather than leaving a spinner.
        if (open) {
          settleStepItems(open);
          open.status = "done";
        }
        open = {
          id: id("step"),
          label: str(d.label) ?? "Step",
          summary: str(d.summary),
          status: "running",
          items: []
        };
        steps.push(open);
        break;
      }
      case "step_end": {
        if (!open) break;
        const outcome = str(d.outcome);
        open.status =
          outcome === "failed" ? "failed" : outcome === "suspended" ? "suspended" : "done";
        settleStepItems(open, open.status === "failed");
        open = null;
        break;
      }
      case "step_summary_update": {
        const summary = str(d.summary);
        if (open && summary) open.summary = summary;
        break;
      }

      case "tool_call": {
        const name = str(d.name) ?? "tool";
        ensureStep().items.push({
          id: id("tool"),
          kind: "tool",
          label: prettyToolName(name),
          preview: inputPreview(d.input),
          input: prettyPayload(d.input),
          running: true
        });
        break;
      }
      case "tool_result": {
        const item = lastRunning("tool");
        if (item) {
          item.running = false;
          item.error = d.is_error === true;
          item.output = prettyPayload(d.output);
          const ms = num(d.duration_ms);
          if (ms !== undefined) item.detail = formatMs(ms);
        }
        break;
      }

      case "thinking_start":
        ensureStep().items.push({
          id: id("think"),
          kind: "thinking",
          label: "Thinking",
          text: "",
          running: true
        });
        break;
      case "thinking_token": {
        const item = lastRunning("thinking");
        if (item) item.text = (item.text ?? "") + (str(d.token) ?? "");
        break;
      }
      case "thinking_end": {
        const item = lastRunning("thinking");
        if (item) item.running = false;
        break;
      }

      case "semantic_shortcut_attempted":
        ensureStep().items.push({
          id: id("sem"),
          kind: "tool",
          label: "Compile semantic query",
          input: prettyPayload(d),
          running: true
        });
        break;
      case "semantic_shortcut_resolved": {
        const item = lastRunning("tool");
        if (item) {
          item.running = false;
          item.output = prettyPayload(d.sql ?? d);
        }
        break;
      }

      case "query_executed": {
        const failed = d.success === false;
        ensureStep().items.push({
          id: id("sql"),
          kind: "sql",
          label: failed ? "Query failed" : "Query",
          detail: failed ? (str(d.error) ?? undefined) : sqlDetail(d),
          input: prettyPayload(d.query ?? d.sql),
          output: failed ? prettyPayload(d.error) : undefined,
          error: failed
        });
        break;
      }
      case "verified_sql":
        ensureStep().items.push({
          id: id("sql"),
          kind: "sql",
          label: "Verified query",
          detail: sqlDetail(d),
          input: prettyPayload(d.query ?? d.sql)
        });
        break;

      case "schema_resolved": {
        const tables = Array.isArray(d.tables) ? d.tables.length : undefined;
        ensureStep().items.push({
          id: id("schema"),
          kind: "note",
          label: "Schema resolved",
          detail: tables ? `${tables} table${tables === 1 ? "" : "s"}` : undefined
        });
        break;
      }

      case "recovery_resumed": {
        if (open) {
          settleStepItems(open);
          open.status = "done";
          open = null;
        }
        steps.push({
          id: id("step"),
          label: "Resuming",
          summary: str(d.message) ?? "Resuming from server restart",
          status: "done",
          items: []
        });
        break;
      }

      case "awaiting_input": {
        // Suspension for a clarifying question — close the open step so its
        // tool/SQL rows stop spinning while the run waits on the user.
        if (open) {
          settleStepItems(open);
          open.status = "suspended";
          open = null;
        }
        break;
      }

      case "error":
      case "failed": {
        if (open) {
          settleStepItems(open, true);
          open.status = "failed";
          open = null;
        }
        break;
      }
      case "done":
      case "cancelled": {
        if (open) {
          settleStepItems(open);
          open.status = "done";
          open = null;
        }
        break;
      }
      default:
        break;
    }
  }
  return steps;
}
