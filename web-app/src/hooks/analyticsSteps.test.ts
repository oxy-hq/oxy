import { describe, expect, it } from "vitest";
import type { UiBlock } from "@/services/api/analytics";
import { buildAnalyticsSteps } from "./analyticsSteps";

// ── helpers ───────────────────────────────────────────────────────────────────

let seq = 0;
const ev = <T extends UiBlock["event_type"]>(
  type: T,
  payload: Extract<UiBlock, { event_type: T }>["payload"]
): UiBlock => ({ seq: seq++, event_type: type, payload }) as UiBlock;

const stepStart = (label: string, subSpecIndex?: number) =>
  ev("step_start", { label, ...(subSpecIndex != null ? { sub_spec_index: subSpecIndex } : {}) });
const stepEnd = (outcome: "advanced" | "failed" = "advanced", subSpecIndex?: number) =>
  ev("step_end", {
    label: "",
    outcome,
    ...(subSpecIndex != null ? { sub_spec_index: subSpecIndex } : {})
  });
const fanOutStart = (total: number) => ev("fan_out_start", { total });
const subSpecStart = (index: number, label: string) =>
  ev("sub_spec_start", { index, total: 0, label });
const subSpecEnd = (index: number, success = true) => ev("sub_spec_end", { index, success });
const fanOutEnd = () => ev("fan_out_end", { success: true });
const queryExecuted = (sql = "SELECT 1", source: "semantic" | "llm" | "vendor" = "llm") =>
  ev("query_executed", {
    query: sql,
    row_count: 1,
    duration_ms: 10,
    success: true,
    columns: ["id"],
    rows: [["1"]],
    source
  });

// ── basic step behaviour ──────────────────────────────────────────────────────

describe("buildAnalyticsSteps — basic steps", () => {
  it("returns empty for no events", () => {
    expect(buildAnalyticsSteps([])).toEqual([]);
  });

  it("open step (streaming) appears in result", () => {
    const items = buildAnalyticsSteps([stepStart("Analyzing")]);
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ kind: "step", label: "Analyzing", isStreaming: true });
  });

  it("closed step is marked not streaming", () => {
    const items = buildAnalyticsSteps([stepStart("Analyzing"), stepEnd()]);
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ kind: "step", label: "Analyzing", isStreaming: false });
  });

  it("failed step_end sets error", () => {
    const items = buildAnalyticsSteps([stepStart("Solving"), stepEnd("failed")]);
    expect(items[0]).toMatchObject({ kind: "step", error: "Step failed" });
  });

  it("query_executed source is propagated to SqlItem", () => {
    const steps = buildAnalyticsSteps([
      stepStart("Executing"),
      queryExecuted("SELECT 1", "semantic"),
      stepEnd()
    ]);
    const step = steps[0] as { items: { kind: string; source?: string }[] };
    expect(step.items[0]).toMatchObject({ kind: "sql", source: "semantic" });
  });

  it("sequential steps are all in result", () => {
    const items = buildAnalyticsSteps([
      stepStart("A"),
      stepEnd(),
      stepStart("B"),
      stepEnd(),
      stepStart("C")
    ]);
    expect(items).toHaveLength(3);
    expect(items.map((i) => (i as { label: string }).label)).toEqual(["A", "B", "C"]);
  });

  it("thinking items accumulate inside a step", () => {
    const items = buildAnalyticsSteps([
      stepStart("S"),
      ev("thinking_start", {}),
      ev("thinking_token", { token: "hell" }),
      ev("thinking_token", { token: "o" }),
      ev("thinking_end", {}),
      stepEnd()
    ]);
    const step = items[0] as { items: { kind: string; text: string; isStreaming: boolean }[] };
    expect(step.items).toHaveLength(1);
    expect(step.items[0]).toMatchObject({ kind: "thinking", text: "hello", isStreaming: false });
  });

  it("text_delta tokens append to a single text item", () => {
    const items = buildAnalyticsSteps([
      stepStart("S"),
      ev("text_delta", { token: "foo" }),
      ev("text_delta", { token: "bar" }),
      stepEnd()
    ]);
    const step = items[0] as { items: { kind: string; text: string }[] };
    expect(step.items).toHaveLength(1);
    expect(step.items[0]).toMatchObject({ kind: "text", text: "foobar" });
  });

  it("tool_call is paired with tool_result", () => {
    const items = buildAnalyticsSteps([
      stepStart("S"),
      ev("tool_call", { name: "run_sql", input: { sql: "SELECT 1" } }),
      ev("tool_result", { name: "run_sql", output: { rows: [] }, duration_ms: 42 }),
      stepEnd()
    ]);
    const step = items[0] as { items: { kind: string; toolOutput: string; durationMs: number }[] };
    expect(step.items).toHaveLength(1);
    expect(step.items[0]).toMatchObject({ kind: "artifact", durationMs: 42, isStreaming: false });
  });
});

// ── fan-out: completed stream ──────────────────────────────────────────────────

describe("buildAnalyticsSteps — fan-out (complete)", () => {
  it("fan-out group appears in result after fan_out_end", () => {
    const items = buildAnalyticsSteps([
      fanOutStart(2),
      subSpecStart(0, "Q1"),
      stepStart("Solving", 0),
      stepEnd("advanced", 0),
      subSpecEnd(0),
      subSpecStart(1, "Q2"),
      stepStart("Solving", 1),
      stepEnd("advanced", 1),
      subSpecEnd(1),
      fanOutEnd()
    ]);
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ kind: "fan_out", total: 2, isStreaming: false });
  });

  it("each card contains its own steps", () => {
    const items = buildAnalyticsSteps([
      fanOutStart(2),
      subSpecStart(0, "Q1"),
      stepStart("Solving", 0),
      stepEnd("advanced", 0),
      queryExecuted("SELECT 1"),
      subSpecEnd(0),
      subSpecStart(1, "Q2"),
      stepStart("Solving", 1),
      stepEnd("advanced", 1),
      queryExecuted("SELECT 2"),
      subSpecEnd(1),
      fanOutEnd()
    ]);
    const group = items[0] as { cards: { steps: { kind: string }[]; label: string }[] };
    expect(group.cards).toHaveLength(2);
    expect(group.cards[0].label).toBe("Q1");
    expect(group.cards[0].steps).toHaveLength(1);
    expect(group.cards[1].label).toBe("Q2");
    expect(group.cards[1].steps).toHaveLength(1);
  });

  it("steps that closed inside a card do NOT appear in top-level result", () => {
    const items = buildAnalyticsSteps([
      fanOutStart(1),
      subSpecStart(0, "Q1"),
      stepStart("Solving", 0),
      stepEnd("advanced", 0),
      subSpecEnd(0),
      fanOutEnd()
    ]);
    // Only the fan_out group should be at the top level
    expect(items).toHaveLength(1);
    expect(items[0].kind).toBe("fan_out");
  });

  it("card steps contain domain items when routed via sub_spec_index", () => {
    const items = buildAnalyticsSteps([
      fanOutStart(1),
      subSpecStart(0, "Q1"),
      stepStart("Executing", 0),
      ev("text_delta", { token: "analysis result", sub_spec_index: 0 }),
      stepEnd("advanced", 0),
      subSpecEnd(0),
      fanOutEnd()
    ]);
    const group = items[0] as { cards: { steps: { items: { kind: string }[] }[] }[] };
    const cardStep = group.cards[0].steps[0];
    expect(cardStep.items).toHaveLength(1);
    expect(cardStep.items[0].kind).toBe("text");
  });
});

// ── fan-out: mid-stream (flush) ────────────────────────────────────────────────

describe("buildAnalyticsSteps — fan-out (streaming / flush)", () => {
  it("active card is visible before sub_spec_end (flush path)", () => {
    // sub_spec_start fired, step inside it has already closed, sub_spec_end not yet
    const items = buildAnalyticsSteps([
      fanOutStart(2),
      subSpecStart(0, "Q1"),
      stepStart("Solving", 0),
      stepEnd("advanced", 0)
      // ← sub_spec_end has NOT arrived yet
    ]);
    expect(items).toHaveLength(1);
    expect(items[0].kind).toBe("fan_out");

    const group = items[0] as { cards: { steps: unknown[]; label: string }[] };
    expect(group.cards).toHaveLength(1);
    expect(group.cards[0].label).toBe("Q1");
    expect(group.cards[0].steps).toHaveLength(1);
  });

  it("open step inside streaming card is visible (flush path)", () => {
    const items = buildAnalyticsSteps([
      fanOutStart(1),
      subSpecStart(0, "Q1"),
      stepStart("Solving", 0)
      // step_end has NOT arrived
    ]);
    const group = items[0] as { cards: { steps: { isStreaming: boolean }[] }[] };
    expect(group.cards[0].steps).toHaveLength(1);
    expect(group.cards[0].steps[0].isStreaming).toBe(true);
  });

  it("no sub_spec_start yet: fan_out group has no cards", () => {
    const items = buildAnalyticsSteps([fanOutStart(3)]);
    expect(items).toHaveLength(1);
    const group = items[0] as { cards: unknown[] };
    expect(group.cards).toHaveLength(0);
  });

  it("completed card + in-progress card both visible while streaming", () => {
    const items = buildAnalyticsSteps([
      fanOutStart(2),
      subSpecStart(0, "Q1"),
      stepStart("Solving", 0),
      stepEnd("advanced", 0),
      subSpecEnd(0),
      subSpecStart(1, "Q2"),
      stepStart("Solving", 1)
      // Q2's step_end and sub_spec_end not yet received
    ]);
    const group = items[0] as { cards: { label: string; steps: unknown[] }[] };
    expect(group.cards).toHaveLength(2);
    expect(group.cards[0].label).toBe("Q1");
    expect(group.cards[0].steps).toHaveLength(1);
    expect(group.cards[1].label).toBe("Q2");
    expect(group.cards[1].steps).toHaveLength(1); // open step flushed into card
  });
});

// ── procedure steps ───────────────────────────────────────────────────────────

describe("buildAnalyticsSteps — procedure steps", () => {
  it("procedure_step_started creates a streaming artifact with toolInput 'Running…'", () => {
    const items = buildAnalyticsSteps([
      stepStart("Executing"),
      ev("procedure_step_started", { step: "Run monthly report" })
    ]);
    const step = items[0] as {
      items: { kind: string; toolName: string; toolInput: string; isStreaming: boolean }[];
    };
    expect(step.items).toHaveLength(1);
    expect(step.items[0]).toMatchObject({
      kind: "artifact",
      toolName: "Run monthly report",
      toolInput: "Running\u2026",
      isStreaming: true
    });
  });

  it("procedure_step_completed (success) closes the item with 'Completed'", () => {
    const items = buildAnalyticsSteps([
      stepStart("Executing"),
      ev("procedure_step_started", { step: "Run monthly report" }),
      ev("procedure_step_completed", { step: "Run monthly report", success: true })
    ]);
    const step = items[0] as { items: { toolOutput: string; isStreaming: boolean }[] };
    expect(step.items[0]).toMatchObject({ toolOutput: "Completed", isStreaming: false });
  });

  it("procedure_step_completed (failure) sets error string as toolOutput", () => {
    const items = buildAnalyticsSteps([
      stepStart("Executing"),
      ev("procedure_step_started", { step: "Run monthly report" }),
      ev("procedure_step_completed", {
        step: "Run monthly report",
        success: false,
        error: "Connection refused"
      })
    ]);
    const step = items[0] as { items: { toolOutput: string; isStreaming: boolean }[] };
    expect(step.items[0]).toMatchObject({ toolOutput: "Connection refused", isStreaming: false });
  });

  it("procedure_step_completed with no error message falls back to 'Failed'", () => {
    const items = buildAnalyticsSteps([
      stepStart("S"),
      ev("procedure_step_started", { step: "Step A" }),
      ev("procedure_step_completed", { step: "Step A", success: false })
    ]);
    const step = items[0] as { items: { toolOutput: string }[] };
    expect(step.items[0]).toMatchObject({ toolOutput: "Failed" });
  });

  it("multiple concurrent steps are independently paired by name", () => {
    const items = buildAnalyticsSteps([
      stepStart("S"),
      ev("procedure_step_started", { step: "Step A" }),
      ev("procedure_step_started", { step: "Step B" }),
      ev("procedure_step_completed", { step: "Step A", success: true }),
      ev("procedure_step_completed", { step: "Step B", success: false, error: "oops" })
    ]);
    const step = items[0] as {
      items: { toolName: string; toolOutput: string; isStreaming: boolean }[];
    };
    expect(step.items).toHaveLength(2);
    expect(step.items[0]).toMatchObject({
      toolName: "Step A",
      toolOutput: "Completed",
      isStreaming: false
    });
    expect(step.items[1]).toMatchObject({
      toolName: "Step B",
      toolOutput: "oops",
      isStreaming: false
    });
  });

  it("unmatched procedure_step_completed (no prior start) is a no-op", () => {
    const items = buildAnalyticsSteps([
      stepStart("S"),
      ev("procedure_step_completed", { step: "Ghost step", success: true })
    ]);
    const step = items[0] as { items: unknown[] };
    expect(step.items).toHaveLength(0);
  });

  it("still-streaming step (no completed yet) stays streaming with no toolOutput", () => {
    const items = buildAnalyticsSteps([
      stepStart("S"),
      ev("procedure_step_started", { step: "Long running step" })
    ]);
    const step = items[0] as { items: { toolOutput: unknown; isStreaming: boolean }[] };
    expect(step.items[0]).toMatchObject({ isStreaming: true });
    expect(step.items[0].toolOutput).toBeUndefined();
  });
});

// ── procedure item stepsDone tracking ─────────────────────────────────────────

type ProcedureItemShape = {
  kind: "procedure";
  procedureName: string;
  steps: { name: string; task_type: string }[];
  stepsDone: number;
  isStreaming: boolean;
};

const MAIN_STEPS = [
  { name: "fetch_data", task_type: "execute_sql" },
  { name: "process_data", task_type: "execute_sql" },
  { name: "generate_report", task_type: "formatter" }
];

const procedureStarted = (name = "my_proc", steps = MAIN_STEPS) =>
  ev("procedure_started", { procedure_name: name, steps });

const procStepStarted = (step: string) => ev("procedure_step_started", { step });

const procStepCompleted = (step: string, success = true, error?: string) =>
  ev("procedure_step_completed", { step, success, ...(error ? { error } : {}) });

const procCompleted = (name = "my_proc", success = true) =>
  ev("procedure_completed", { procedure_name: name, success });

const getProcItem = (items: ReturnType<typeof buildAnalyticsSteps>): ProcedureItemShape => {
  for (const node of items) {
    if (node.kind !== "step") continue;
    const found = (node as { items: unknown[] }).items.find(
      (i) => (i as { kind: string }).kind === "procedure"
    );
    if (found) return found as ProcedureItemShape;
  }
  throw new Error("no procedure item found");
};

describe("buildAnalyticsSteps — procedure item stepsDone", () => {
  it("starts at 0 with no step events", () => {
    const items = buildAnalyticsSteps([stepStart("Running"), procedureStarted()]);
    expect(getProcItem(items).stepsDone).toBe(0);
  });

  it("increments for each successful main-step completion", () => {
    const items = buildAnalyticsSteps([
      stepStart("Running"),
      procedureStarted(),
      procStepStarted("fetch_data"),
      procStepCompleted("fetch_data"),
      procStepStarted("process_data"),
      procStepCompleted("process_data")
    ]);
    expect(getProcItem(items).stepsDone).toBe(2);
  });

  it("does NOT increment for a failed main step", () => {
    const items = buildAnalyticsSteps([
      stepStart("Running"),
      procedureStarted(),
      procStepStarted("fetch_data"),
      procStepCompleted("fetch_data", false, "timeout")
    ]);
    expect(getProcItem(items).stepsDone).toBe(0);
  });

  it("does NOT increment for loop_sequential sub-step completions", () => {
    const steps = [{ name: "loop_step", task_type: "loop_sequential" }];
    const items = buildAnalyticsSteps([
      stepStart("Running"),
      procedureStarted("p", steps),
      procStepStarted("loop_step"),
      // sub-steps have names not in the top-level list
      procStepStarted("sub_1"),
      procStepCompleted("sub_1"),
      procStepStarted("sub_2"),
      procStepCompleted("sub_2")
    ]);
    expect(getProcItem(items).stepsDone).toBe(0);
  });

  it("increments only for main-step completions when sub-steps are mixed in", () => {
    const steps = [
      { name: "prepare", task_type: "execute_sql" },
      { name: "loop_step", task_type: "loop_sequential" },
      { name: "finalize", task_type: "execute_sql" }
    ];
    const items = buildAnalyticsSteps([
      stepStart("Running"),
      procedureStarted("p", steps),
      procStepStarted("prepare"),
      procStepCompleted("prepare"), // +1
      procStepStarted("loop_step"),
      procStepStarted("sub_1"),
      procStepCompleted("sub_1"), // sub-step — no increment
      procStepStarted("sub_2"),
      procStepCompleted("sub_2"), // sub-step — no increment
      procStepCompleted("loop_step"), // +1
      procStepStarted("finalize"),
      procStepCompleted("finalize") // +1
    ]);
    expect(getProcItem(items).stepsDone).toBe(3);
  });

  it("reaches total steps when all main steps complete", () => {
    const items = buildAnalyticsSteps([
      stepStart("Running"),
      procedureStarted(),
      procStepStarted("fetch_data"),
      procStepCompleted("fetch_data"),
      procStepStarted("process_data"),
      procStepCompleted("process_data"),
      procStepStarted("generate_report"),
      procStepCompleted("generate_report"),
      procCompleted(),
      stepEnd()
    ]);
    const proc = getProcItem(items);
    expect(proc.stepsDone).toBe(MAIN_STEPS.length);
    expect(proc.isStreaming).toBe(false);
  });

  it("stepsDone is stable after procedure_completed (no further increments)", () => {
    const items = buildAnalyticsSteps([
      stepStart("Running"),
      procedureStarted(),
      procStepStarted("fetch_data"),
      procStepCompleted("fetch_data"),
      procCompleted(),
      // ghost completion after procedure is done — should not increment
      procStepCompleted("fetch_data")
    ]);
    // procedure is now not streaming, but stepsDone should still be 1
    expect(getProcItem(items).stepsDone).toBe(1);
  });
});

// ── outer steps around fan-out ─────────────────────────────────────────────────

describe("buildAnalyticsSteps — outer steps around fan-out", () => {
  it("outer step before fan-out appears in result alongside the group", () => {
    const items = buildAnalyticsSteps([
      stepStart("Outer"),
      fanOutStart(1),
      subSpecStart(0, "Q1"),
      stepStart("Inner", 0),
      stepEnd("advanced", 0),
      subSpecEnd(0),
      fanOutEnd(),
      stepEnd() // outer closes after fan-out
    ]);
    // fan_out group + outer step
    expect(items).toHaveLength(2);
    expect(items[0].kind).toBe("fan_out");
    expect(items[1]).toMatchObject({ kind: "step", label: "Outer" });
  });

  it("outer step items accumulated before fan-out are preserved", () => {
    const items = buildAnalyticsSteps([
      stepStart("Outer"),
      ev("text_delta", { token: "prefix" }),
      fanOutStart(1),
      subSpecStart(0, "Q1"),
      stepStart("Inner", 0),
      stepEnd("advanced", 0),
      subSpecEnd(0),
      fanOutEnd(),
      stepEnd()
    ]);
    const outerStep = items[1] as { items: { kind: string; text: string }[] };
    expect(outerStep.items).toHaveLength(1);
    expect(outerStep.items[0]).toMatchObject({ kind: "text", text: "prefix" });
  });

  it("inner card items do NOT bleed into outer step", () => {
    const items = buildAnalyticsSteps([
      stepStart("Outer"),
      fanOutStart(1),
      subSpecStart(0, "Q1"),
      stepStart("Inner", 0),
      ev("text_delta", { token: "inner-text", sub_spec_index: 0 }),
      stepEnd("advanced", 0),
      subSpecEnd(0),
      fanOutEnd(),
      stepEnd()
    ]);
    const outerStep = items[1] as { items: unknown[] };
    expect(outerStep.items).toHaveLength(0);
  });
});

// ── concurrent fan-out ──────────────────────────────────────────────────────

describe("buildAnalyticsSteps — concurrent fan-out", () => {
  it("interleaved fan-out cards build correctly", () => {
    const items = buildAnalyticsSteps([
      fanOutStart(3),
      subSpecStart(0, "Query 1 of 3"),
      subSpecStart(1, "Query 2 of 3"),
      // Card 0 step
      stepStart("solving", 0),
      stepStart("solving", 1),
      // Card 1 finishes first
      stepEnd("advanced", 1),
      subSpecEnd(1, true),
      stepEnd("advanced", 0),
      subSpecEnd(0, true),
      subSpecStart(2, "Query 3 of 3"),
      stepStart("solving", 2),
      stepEnd("advanced", 2),
      subSpecEnd(2, true),
      fanOutEnd()
    ]);

    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ kind: "fan_out", isStreaming: false });

    const group = items[0] as { cards: { label: string; steps: { label: string }[] }[] };
    expect(group.cards).toHaveLength(3);

    // Card 1 closed first, so it appears first in cards array
    expect(group.cards[0].label).toBe("Query 2 of 3");
    expect(group.cards[0].steps).toHaveLength(1);
    expect(group.cards[0].steps[0].label).toBe("solving");

    expect(group.cards[1].label).toBe("Query 1 of 3");
    expect(group.cards[1].steps).toHaveLength(1);
    expect(group.cards[1].steps[0].label).toBe("solving");

    expect(group.cards[2].label).toBe("Query 3 of 3");
    expect(group.cards[2].steps).toHaveLength(1);
    expect(group.cards[2].steps[0].label).toBe("solving");
  });

  it("concurrent cards mid-stream flush", () => {
    // Open 2 cards but don't close them, then flush
    const items = buildAnalyticsSteps([
      fanOutStart(3),
      subSpecStart(0, "Card A"),
      stepStart("analyzing", 0),
      subSpecStart(1, "Card B"),
      stepStart("analyzing", 1)
      // Neither card is closed — flush happens at end of buildAnalyticsSteps
    ]);

    expect(items).toHaveLength(1);
    expect(items[0].kind).toBe("fan_out");

    const group = items[0] as {
      cards: { label: string; isStreaming: boolean; steps: { isStreaming: boolean }[] }[];
    };
    expect(group.cards).toHaveLength(2);

    // Both cards should be streaming
    expect(group.cards[0].isStreaming).toBe(true);
    expect(group.cards[0].steps).toHaveLength(1);
    expect(group.cards[0].steps[0].isStreaming).toBe(true);

    expect(group.cards[1].isStreaming).toBe(true);
    expect(group.cards[1].steps).toHaveLength(1);
    expect(group.cards[1].steps[0].isStreaming).toBe(true);
  });

  it("events without sub_spec_index go to outer scope", () => {
    const items = buildAnalyticsSteps([
      stepStart("Before fan-out"),
      ev("text_delta", { token: "before" }),
      stepEnd(),
      fanOutStart(1),
      subSpecStart(0, "Q1"),
      stepStart("solving", 0),
      stepEnd("advanced", 0),
      subSpecEnd(0),
      fanOutEnd(),
      stepStart("After fan-out"),
      ev("text_delta", { token: "after" }),
      stepEnd()
    ]);

    // Should have: outer step "Before fan-out", fan_out group, outer step "After fan-out"
    expect(items).toHaveLength(3);

    const kinds = items.map((i) => i.kind);
    expect(kinds).toContain("fan_out");

    const beforeStep = items.find(
      (i) => i.kind === "step" && (i as { label: string }).label === "Before fan-out"
    ) as { kind: string; label: string; items: { kind: string; text: string }[] };
    expect(beforeStep).toBeDefined();
    expect(beforeStep.items).toHaveLength(1);
    expect(beforeStep.items[0]).toMatchObject({ kind: "text", text: "before" });

    const afterStep = items.find(
      (i) => i.kind === "step" && (i as { label: string }).label === "After fan-out"
    ) as { kind: string; label: string; items: { kind: string; text: string }[] };
    expect(afterStep).toBeDefined();
    expect(afterStep.items).toHaveLength(1);
    expect(afterStep.items[0]).toMatchObject({ kind: "text", text: "after" });
  });

  it("backward compatible — serial fan-out still works", () => {
    // Serial pattern: events inside sub_spec use sub_spec_index routing
    // This mirrors the existing serial test pattern but with correct helpers
    const items = buildAnalyticsSteps([
      fanOutStart(2),
      subSpecStart(0, "Q1"),
      stepStart("Solving", 0),
      stepEnd("advanced", 0),
      subSpecEnd(0),
      subSpecStart(1, "Q2"),
      stepStart("Solving", 1),
      stepEnd("advanced", 1),
      subSpecEnd(1),
      fanOutEnd()
    ]);

    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ kind: "fan_out", total: 2, isStreaming: false });

    const group = items[0] as { cards: { label: string; steps: { label: string }[] }[] };
    expect(group.cards).toHaveLength(2);
    expect(group.cards[0].label).toBe("Q1");
    expect(group.cards[0].steps).toHaveLength(1);
    expect(group.cards[0].steps[0].label).toBe("Solving");
    expect(group.cards[1].label).toBe("Q2");
    expect(group.cards[1].steps).toHaveLength(1);
    expect(group.cards[1].steps[0].label).toBe("Solving");
  });
});
