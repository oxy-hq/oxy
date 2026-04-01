import type { UiBlock } from "@/services/api/analytics";

// ── Trace item types ──────────────────────────────────────────────────────────

export type ThinkingItem = {
  kind: "thinking";
  id: string;
  text: string;
  isStreaming: boolean;
};

export type ArtifactItem = {
  kind: "artifact";
  id: string;
  toolName: string;
  toolInput: string;
  toolOutput?: string;
  durationMs?: number;
  isStreaming: boolean;
};

export type SqlItem = {
  kind: "sql";
  id: string;
  sql: string;
  database?: string;
  result?: string[][];
  rowCount?: number;
  durationMs?: number;
  error?: string;
  isStreaming: boolean;
};

export type TextItem = {
  kind: "text";
  id: string;
  text: string;
  isStreaming: boolean;
};

export type ProcedureItem = {
  kind: "procedure";
  id: string;
  procedureName: string;
  steps: Array<{ name: string; task_type: string }>;
  stepsDone: number;
  isStreaming: boolean;
};

export type TraceItem = ThinkingItem | ArtifactItem | SqlItem | TextItem | ProcedureItem;

export type SelectableItem = ArtifactItem | SqlItem | ProcedureItem;

// ── Step / fan-out types ──────────────────────────────────────────────────────

/** Accumulated LLM usage for a step (may span multiple LLM calls). */
export type StepLlmUsage = {
  inputTokens: number;
  outputTokens: number;
  durationMs: number;
};

export type AnalyticsStep = {
  kind: "step";
  id: string;
  label: string;
  /** One-line description of what this step does; updated dynamically via step_summary_update. */
  summary?: string;
  isStreaming: boolean;
  /** Set when the step ended with a non-successful outcome. */
  error?: string;
  /** Set when the step suspended to await human input. */
  suspended?: boolean;
  items: TraceItem[];
  /** Domain state output attached to step_end. */
  metadata?: Record<string, unknown>;
  /** Accumulated LLM usage across all LLM calls in this step. */
  llmUsage?: StepLlmUsage;
};

/** One card within a fan-out group, containing its own pipeline steps. */
export type FanOutCard = {
  id: string;
  index: number;
  label: string;
  steps: AnalyticsStep[];
  isStreaming: boolean;
};

/** A group of parallel sub-spec executions rendered as navigable cards. */
export type FanOutGroup = {
  kind: "fan_out";
  id: string;
  total: number;
  cards: FanOutCard[];
  isStreaming: boolean;
};

export type StepOrGroup = AnalyticsStep | FanOutGroup;

// ── Scope ─────────────────────────────────────────────────────────────────────
//
// Encapsulates all mutable builder state and exposes explicit lifecycle methods.
// stepStack holds only the steps for the *current* scope. When a fan-out card
// begins, outer open steps are saved aside and the stack is cleared; on card end
// they are restored. Completed steps are routed to currentCard or result based
// solely on whether a card is active — no depth arithmetic needed.

function createScope() {
  const result: StepOrGroup[] = [];
  let counter = 0;
  const nextId = (prefix: string) => `${prefix}-${counter++}`;

  // Outer (non-fan-out) step stack.
  const stepStack: AnalyticsStep[] = [];

  // ── Concurrent fan-out state ────────────────────────────────────────────
  // Multiple cards can be active simultaneously during concurrent fan-out.
  // Each card has its own step stack, keyed by sub-spec index.
  let savedOuterSteps: AnalyticsStep[] | null = null;
  const activeCards = new Map<number, { card: FanOutCard; stepStack: AnalyticsStep[] }>();
  let currentFanOut: FanOutGroup | null = null;

  // ── Routing helpers ─────────────────────────────────────────────────────
  // When sub_spec_index is set, route to the card's step stack; otherwise outer.
  const getStepStack = (subSpecIndex?: number | null): AnalyticsStep[] => {
    if (subSpecIndex != null) {
      const entry = activeCards.get(subSpecIndex);
      if (entry) return entry.stepStack;
    }
    return stepStack;
  };

  const currentStep = (subSpecIndex?: number | null) => getStepStack(subSpecIndex).at(-1);

  const pushItem = (item: TraceItem, subSpecIndex?: number | null) => {
    currentStep(subSpecIndex)?.items.push(item);
  };

  const findLastStreaming = <T extends TraceItem>(
    kind: T["kind"],
    subSpecIndex?: number | null
  ): T | undefined => {
    const items = currentStep(subSpecIndex)?.items;
    if (!items) return undefined;
    for (let i = items.length - 1; i >= 0; i--) {
      const item = items[i];
      if (item.kind === kind && item.isStreaming) return item as T;
    }
    return undefined;
  };

  /** Find the last streaming artifact item whose toolName matches `name`. */
  const findLastStreamingArtifactByName = (
    name: string,
    subSpecIndex?: number | null
  ): ArtifactItem | undefined => {
    const items = currentStep(subSpecIndex)?.items;
    if (!items) return undefined;
    for (let i = items.length - 1; i >= 0; i--) {
      const item = items[i];
      if (item.kind === "artifact" && item.isStreaming && item.toolName === name) {
        return item;
      }
    }
    return undefined;
  };

  const completeStep = (
    outcome: string,
    metadata?: Record<string, unknown> | null,
    subSpecIndex?: number | null
  ) => {
    const stack = getStepStack(subSpecIndex);
    const step = stack.pop();
    if (!step) return;
    step.isStreaming = false;
    if (outcome === "suspended") {
      step.suspended = true;
    } else if (outcome !== "advanced" && outcome !== "retry") {
      step.error = "Step failed";
    }
    if (metadata && !Array.isArray(metadata)) step.metadata = metadata;
    // Route completed step: if inside a card, push to card.steps; else to result.
    if (subSpecIndex != null) {
      const entry = activeCards.get(subSpecIndex);
      if (entry) {
        entry.card.steps.push(step);
        return;
      }
    }
    // Legacy serial path: single currentCard
    if (activeCards.size === 0 && savedOuterSteps) {
      // We're in a serial fan-out (no sub_spec_index routing)
      // This shouldn't happen in the new concurrent model,
      // but keep backward compat.
    }
    result.push(step);
  };

  return {
    nextId,
    pushItem,
    findLastStreaming,
    findLastStreamingArtifactByName,

    // ── Step lifecycle ───────────────────────────────────────────────────────
    openStep(label: string, summary?: string, subSpecIndex?: number | null) {
      getStepStack(subSpecIndex).push({
        kind: "step",
        id: nextId("step"),
        label,
        summary,
        isStreaming: true,
        items: []
      });
    },
    closeStep: completeStep,
    updateSummary(summary: string, subSpecIndex?: number | null) {
      const top = currentStep(subSpecIndex);
      if (top) top.summary = summary;
    },

    // ── LLM usage accumulation ──────────────────────────────────────────────
    accumulateLlmUsage(
      promptTokens: number,
      outputTokens: number,
      durationMs: number,
      subSpecIndex?: number | null
    ) {
      const step = currentStep(subSpecIndex);
      if (!step) return;
      if (!step.llmUsage) {
        step.llmUsage = { inputTokens: 0, outputTokens: 0, durationMs: 0 };
      }
      step.llmUsage.inputTokens += promptTokens || 0;
      step.llmUsage.outputTokens += outputTokens || 0;
      step.llmUsage.durationMs += durationMs || 0;
    },

    // ── Text streaming ───────────────────────────────────────────────────────
    appendTextDelta(token: string, subSpecIndex?: number | null) {
      const last = currentStep(subSpecIndex)?.items.at(-1);
      if (last?.kind === "text" && last.isStreaming) {
        last.text += token;
      } else {
        pushItem({ kind: "text", id: nextId("txt"), text: token, isStreaming: true }, subSpecIndex);
      }
    },

    // ── Fan-out lifecycle ────────────────────────────────────────────────────
    openFanOut(total: number) {
      currentFanOut = {
        kind: "fan_out",
        id: nextId("fan-out"),
        total,
        cards: [],
        isStreaming: true
      };
    },
    openCard(index: number, label: string) {
      // On first card, save the outer step stack.
      if (activeCards.size === 0) {
        savedOuterSteps = [...stepStack];
        stepStack.length = 0;
      }
      activeCards.set(index, {
        card: { id: nextId("card"), index, label, steps: [], isStreaming: true },
        stepStack: []
      });
    },
    closeCard(index: number, success: boolean) {
      const entry = activeCards.get(index);
      if (!entry) return;
      // Drain any open steps on this card's stack.
      while (entry.stepStack.length > 0) {
        const step = entry.stepStack.pop();
        if (!step) break;
        step.isStreaming = false;
        if (!success) step.error = "Step failed";
        entry.card.steps.push(step);
      }
      entry.card.isStreaming = false;
      currentFanOut?.cards.push(entry.card);
      activeCards.delete(index);
      // When all cards are closed, restore outer steps.
      if (activeCards.size === 0) {
        stepStack.push(...(savedOuterSteps ?? []));
        savedOuterSteps = null;
      }
    },
    closeFanOut() {
      if (!currentFanOut) return;
      currentFanOut.isStreaming = false;
      result.push(currentFanOut);
      currentFanOut = null;
    },

    // ── Flush ────────────────────────────────────────────────────────────────
    // Drains any open state left by a still-streaming response.
    flush(): StepOrGroup[] {
      // Drain all active cards.
      for (const [, entry] of activeCards) {
        for (const step of entry.stepStack.splice(0).reverse()) entry.card.steps.push(step);
        currentFanOut?.cards.push(entry.card);
      }
      activeCards.clear();
      if (savedOuterSteps) {
        stepStack.push(...savedOuterSteps);
        savedOuterSteps = null;
      }
      if (currentFanOut) result.push(currentFanOut);
      for (const step of stepStack.splice(0).reverse()) result.push(step);
      return result;
    }
  };
}

// ── Domain item construction ──────────────────────────────────────────────────
//
// Pure function: maps a domain event to the TraceItem it should produce.
// Has no access to scope state — add new domain events here without touching
// the main dispatcher.

function buildDomainItem(ev: UiBlock, nextId: (prefix: string) => string): TraceItem | null {
  switch (ev.event_type) {
    case "schema_resolved": {
      const tables = ev.payload.tables ?? [];
      return {
        kind: "artifact",
        id: nextId("schema"),
        toolName: "resolve_schema",
        toolInput: "",
        toolOutput: tables.length ? `Tables: ${tables.join(", ")}` : "Schema resolved",
        durationMs: ev.payload.duration_ms,
        isStreaming: false
      };
    }

    case "triage_completed":
      return {
        kind: "text",
        id: nextId("triage"),
        text: ev.payload.summary ?? "Triage completed",
        isStreaming: false
      };

    case "intent_clarified": {
      const parts: string[] = [];
      if (ev.payload.metrics?.length) parts.push(`Metrics: ${ev.payload.metrics.join(", ")}`);
      if (ev.payload.dimensions?.length)
        parts.push(`Dimensions: ${ev.payload.dimensions.join(", ")}`);
      if (ev.payload.filters?.length) parts.push(`Filters: ${ev.payload.filters.join(", ")}`);
      return {
        kind: "text",
        id: nextId("intent"),
        text: parts.join(" · ") || "Intent clarified",
        isStreaming: false
      };
    }

    case "spec_resolved": {
      const metrics = ev.payload.resolved_metrics ?? [];
      const tables = ev.payload.resolved_tables ?? [];
      const parts: string[] = [];
      if (metrics.length) parts.push(`Metrics: ${metrics.join(", ")}`);
      if (tables.length) parts.push(`Tables: ${tables.join(", ")}`);
      return {
        kind: "text",
        id: nextId("spec"),
        text: parts.join(" · ") || "Spec resolved",
        isStreaming: false
      };
    }

    case "query_generated":
      return { kind: "sql", id: nextId("sql"), sql: ev.payload.sql, isStreaming: false };

    case "query_executed":
      return {
        kind: "sql",
        id: nextId("sql"),
        sql: ev.payload.query,
        isStreaming: false,
        rowCount: ev.payload.row_count,
        durationMs: ev.payload.duration_ms,
        error: ev.payload.success ? undefined : (ev.payload.error ?? "unknown error"),
        result: [ev.payload.columns].concat(ev.payload.rows ?? [])
      };

    case "analytics_validation_failed":
      return {
        kind: "text",
        id: nextId("val"),
        text: ev.payload.reason ?? "Validation failed",
        isStreaming: false
      };

    // ── App-builder domain events ─────────────────────────────────────────
    case "task_plan_ready":
      return {
        kind: "text",
        id: nextId("plan"),
        text: `Plan ready: ${ev.payload.task_count ?? 0} tasks, ${ev.payload.control_count ?? 0} controls`,
        isStreaming: false
      };

    case "task_sql_resolved":
      return {
        kind: "sql",
        id: nextId("sql-ok"),
        sql: ev.payload.sql ?? "",
        isStreaming: false
      };

    case "task_executed":
      return {
        kind: "sql",
        id: nextId("exec-ok"),
        sql: ev.payload.sql ?? "",
        rowCount: ev.payload.row_count,
        result:
          ev.payload.columns && ev.payload.sample_rows
            ? [ev.payload.columns, ...(ev.payload.sample_rows ?? [])]
            : undefined,
        isStreaming: false
      };

    case "app_yaml_ready":
      return {
        kind: "text",
        id: nextId("yaml"),
        text: "App YAML generated",
        isStreaming: false
      };

    case "llm_usage": {
      const parts: string[] = [];
      if (ev.payload.prompt_tokens) parts.push(`${ev.payload.prompt_tokens} input tokens`);
      if (ev.payload.output_tokens) parts.push(`${ev.payload.output_tokens} output tokens`);
      if (ev.payload.duration_ms) parts.push(`${(ev.payload.duration_ms / 1000).toFixed(1)}s`);
      return parts.length
        ? {
            kind: "text",
            id: nextId("usage"),
            text: `LLM: ${parts.join(", ")}`,
            isStreaming: false
          }
        : null;
    }

    default:
      return null;
  }
}

// ── Entry point ───────────────────────────────────────────────────────────────

export function buildAnalyticsSteps(events: UiBlock[]): StepOrGroup[] {
  const scope = createScope();

  for (const ev of events) {
    // Extract sub_spec_index for routing events to the correct card.
    const ssi =
      "sub_spec_index" in ev.payload
        ? (ev.payload as { sub_spec_index?: number | null }).sub_spec_index
        : undefined;

    switch (ev.event_type) {
      // ── Step lifecycle ─────────────────────────────────────────────────────
      case "step_start":
        scope.openStep(ev.payload.label, ev.payload.summary ?? undefined, ssi);
        break;
      case "step_end":
        scope.closeStep(ev.payload.outcome, ev.payload.metadata, ssi);
        break;
      case "step_summary_update":
        scope.updateSummary(ev.payload.summary, ssi);
        break;

      // ── Thinking stream ────────────────────────────────────────────────────
      case "thinking_start":
        scope.pushItem(
          {
            kind: "thinking",
            id: scope.nextId("think"),
            text: "",
            isStreaming: true
          },
          ssi
        );
        break;
      case "thinking_token": {
        const t = scope.findLastStreaming<ThinkingItem>("thinking", ssi);
        if (t) t.text += ev.payload.token;
        break;
      }
      case "thinking_end": {
        const t = scope.findLastStreaming<ThinkingItem>("thinking", ssi);
        if (t) t.isStreaming = false;
        break;
      }

      // ── Tool calls ─────────────────────────────────────────────────────────
      case "tool_call":
        scope.pushItem(
          {
            kind: "artifact",
            id: scope.nextId("tool"),
            toolName: ev.payload.name,
            toolInput: JSON.stringify(ev.payload.input ?? ""),
            isStreaming: true
          },
          ssi
        );
        break;
      case "tool_result": {
        const a = scope.findLastStreaming<ArtifactItem>("artifact", ssi);
        if (a) {
          a.toolOutput = JSON.stringify(ev.payload.output ?? "");
          a.durationMs = ev.payload.duration_ms;
          a.isStreaming = false;
        }
        break;
      }

      // ── Streaming text ─────────────────────────────────────────────────────
      case "text_delta":
        scope.appendTextDelta(ev.payload.token, ssi);
        break;

      // ── Fan-out lifecycle ──────────────────────────────────────────────────
      case "fan_out_start":
        scope.openFanOut(ev.payload.total);
        break;
      case "sub_spec_start":
        scope.openCard(ev.payload.index, ev.payload.label);
        break;
      case "sub_spec_end":
        scope.closeCard(ev.payload.index, ev.payload.success);
        break;
      case "fan_out_end":
        scope.closeFanOut();
        break;

      // ── Procedure lifecycle ────────────────────────────────────────────────
      case "procedure_started":
        scope.pushItem({
          kind: "procedure",
          id: scope.nextId("proc-run"),
          procedureName: ev.payload.procedure_name,
          steps: ev.payload.steps,
          stepsDone: 0,
          isStreaming: true
        });
        break;

      case "procedure_completed": {
        const p = scope.findLastStreaming<ProcedureItem>("procedure");
        if (p) p.isStreaming = false;
        break;
      }

      // ── Procedure execution progress ───────────────────────────────────────
      case "procedure_step_started":
        scope.pushItem({
          kind: "artifact",
          id: scope.nextId("proc-step"),
          toolName: ev.payload.step,
          toolInput: "Running\u2026",
          isStreaming: true
        });
        break;

      case "procedure_step_completed": {
        // Update the paired streaming artifact by step name
        const artifact = scope.findLastStreamingArtifactByName(ev.payload.step);
        if (artifact) {
          artifact.isStreaming = false;
          artifact.toolOutput = ev.payload.success ? "Completed" : (ev.payload.error ?? "Failed");
        }
        // Also update stepsDone on the procedure item
        if (ev.payload.success) {
          const p = scope.findLastStreaming<ProcedureItem>("procedure");
          if (p?.steps.some((s) => s.name === ev.payload.step)) p.stepsDone += 1;
        }
        break;
      }

      // ── LLM usage accumulation ──────────────────────────────────────────────
      case "llm_usage":
        scope.accumulateLlmUsage(
          ev.payload.prompt_tokens,
          ev.payload.output_tokens,
          ev.payload.duration_ms,
          ssi
        );
        break;

      // ── Domain events (pure item construction) ─────────────────────────────
      default: {
        const item = buildDomainItem(ev, scope.nextId);
        if (item) scope.pushItem(item, ssi);
      }
    }
  }

  return scope.flush();
}
