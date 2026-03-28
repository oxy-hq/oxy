// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SseEvent } from "@/hooks/useAnalyticsRun";
import ProcedureRunDagPanel from "./index";

// ── Event helpers ─────────────────────────────────────────────────────────────

let counter = 0;

const sseEv = <T extends SseEvent["type"]>(
  type: T,
  data: Extract<SseEvent, { type: T }>["data"]
): SseEvent => ({ id: String(counter++), type, data }) as SseEvent;

const procedureStarted = (name: string, steps: Array<{ name: string; task_type: string }>) =>
  sseEv("procedure_started", { procedure_name: name, steps });

const procedureCompleted = (name: string, success: boolean, error?: string) =>
  sseEv("procedure_completed", {
    procedure_name: name,
    success,
    ...(error ? { error } : {})
  } as Extract<SseEvent, { type: "procedure_completed" }>["data"]);

const stepStarted = (step: string) => sseEv("procedure_step_started", { step });

const stepCompleted = (step: string, success: boolean, error?: string) =>
  sseEv("procedure_step_completed", {
    step,
    success,
    ...(error ? { error } : {})
  } as Extract<SseEvent, { type: "procedure_step_completed" }>["data"]);

// ── Constants ─────────────────────────────────────────────────────────────────

const STEPS = [
  { name: "fetch_data", task_type: "execute_sql" },
  { name: "process_data", task_type: "execute_sql" },
  { name: "generate_report", task_type: "formatter" }
];
const noop = () => {};

beforeEach(() => {
  counter = 0;
});
afterEach(() => {
  cleanup();
});

// ── Rendering ─────────────────────────────────────────────────────────────────

describe("ProcedureRunDagPanel — rendering", () => {
  it("renders all step names", () => {
    render(
      <ProcedureRunDagPanel
        procedureName="my_procedure"
        steps={STEPS}
        events={[]}
        isRunning={false}
        onClose={noop}
      />
    );
    for (const step of STEPS) {
      expect(screen.getByText(step.name)).toBeInTheDocument();
    }
  });

  it("renders procedure name in the header", () => {
    render(
      <ProcedureRunDagPanel
        procedureName="my_procedure"
        steps={STEPS}
        events={[]}
        isRunning={false}
        onClose={noop}
      />
    );
    expect(screen.getByText("my_procedure")).toBeInTheDocument();
  });

  it("falls back to 'Procedure Run' when procedure name is empty", () => {
    render(
      <ProcedureRunDagPanel
        procedureName=""
        steps={STEPS}
        events={[]}
        isRunning={false}
        onClose={noop}
      />
    );
    expect(screen.getByText("Procedure Run")).toBeInTheDocument();
  });

  it("renders nothing beyond the header when steps is empty", () => {
    const { container } = render(
      <ProcedureRunDagPanel
        procedureName="p"
        steps={[]}
        events={[]}
        isRunning={false}
        onClose={noop}
      />
    );
    // No step node divs — only the panel shell remains
    const content = container.querySelector("[data-slot='panel-content']");
    expect(content?.textContent?.trim()).toBe("");
  });

  it("renders one node per step", () => {
    render(
      <ProcedureRunDagPanel
        procedureName="p"
        steps={[
          { name: "a", task_type: "execute_sql" },
          { name: "b", task_type: "execute_sql" },
          { name: "c", task_type: "execute_sql" }
        ]}
        events={[]}
        isRunning={false}
        onClose={noop}
      />
    );
    expect(screen.getByText("a")).toBeInTheDocument();
    expect(screen.getByText("b")).toBeInTheDocument();
    expect(screen.getByText("c")).toBeInTheDocument();
  });
});

// ── Header subtitle ───────────────────────────────────────────────────────────

describe("ProcedureRunDagPanel — header subtitle", () => {
  it("shows 'Running…' subtitle when isRunning is true and no events", () => {
    render(
      <ProcedureRunDagPanel
        procedureName="p"
        steps={STEPS}
        events={[]}
        isRunning={true}
        onClose={noop}
      />
    );
    const header = screen.getByRole("heading", { name: "p" }).closest("[data-slot='panel-header']");
    expect(within(header as HTMLElement).getByText("Running…")).toBeInTheDocument();
  });

  it("shows 'Completed' subtitle when procedure_completed success=true", () => {
    render(
      <ProcedureRunDagPanel
        procedureName="p"
        steps={STEPS}
        events={[procedureStarted("p", STEPS), procedureCompleted("p", true)]}
        isRunning={false}
        onClose={noop}
      />
    );
    const header = screen.getByRole("heading", { name: "p" }).closest("[data-slot='panel-header']");
    expect(within(header as HTMLElement).getByText("Completed")).toBeInTheDocument();
  });

  it("shows 'Failed' subtitle when procedure_completed success=false", () => {
    render(
      <ProcedureRunDagPanel
        procedureName="p"
        steps={STEPS}
        events={[procedureStarted("p", STEPS), procedureCompleted("p", false, "timeout")]}
        isRunning={false}
        onClose={noop}
      />
    );
    const header = screen.getByRole("heading", { name: "p" }).closest("[data-slot='panel-header']");
    expect(within(header as HTMLElement).getByText("Failed")).toBeInTheDocument();
  });

  it("renders no subtitle when not running and no procedure_completed event", () => {
    const { container } = render(
      <ProcedureRunDagPanel
        procedureName="p"
        steps={STEPS}
        events={[]}
        isRunning={false}
        onClose={noop}
      />
    );
    const subtitle = container.querySelector("[data-slot='panel-header'] p");
    expect(subtitle).toBeNull();
  });

  it("isRunning subtitle takes precedence over a completed event when still running", () => {
    // Edge case: both isRunning=true and procedure_completed in events
    render(
      <ProcedureRunDagPanel
        procedureName="p"
        steps={STEPS}
        events={[procedureCompleted("p", true)]}
        isRunning={true}
        onClose={noop}
      />
    );
    const header = screen.getByRole("heading", { name: "p" }).closest("[data-slot='panel-header']");
    expect(within(header as HTMLElement).getByText("Running…")).toBeInTheDocument();
    expect(within(header as HTMLElement).queryByText("Completed")).toBeNull();
  });
});

// ── Step statuses ─────────────────────────────────────────────────────────────

describe("ProcedureRunDagPanel — step statuses", () => {
  it("all steps are idle (no status label) when no step events", () => {
    const { container } = render(
      <ProcedureRunDagPanel
        procedureName="p"
        steps={[{ name: "step_a", task_type: "execute_sql" }, { name: "step_b", task_type: "execute_sql" }]}
        events={[]}
        isRunning={false}
        onClose={noop}
      />
    );
    const content = container.querySelector("[data-slot='panel-content']");
    expect(within(content as HTMLElement).queryByText("Running…")).toBeNull();
    expect(within(content as HTMLElement).queryByText("Done")).toBeNull();
    expect(within(content as HTMLElement).queryByText("Failed")).toBeNull();
  });

  it("procedure_step_started marks the target step as running", () => {
    const { container } = render(
      <ProcedureRunDagPanel
        procedureName="p"
        steps={[{ name: "step_a", task_type: "execute_sql" }, { name: "step_b", task_type: "execute_sql" }]}
        events={[stepStarted("step_a")]}
        isRunning={false}
        onClose={noop}
      />
    );
    const content = container.querySelector("[data-slot='panel-content']") as HTMLElement;
    expect(within(content).getByText("Running…")).toBeInTheDocument();
    // Only one step label visible — step_b is still idle
    expect(within(content).queryAllByText("Done")).toHaveLength(0);
  });

  it("procedure_step_completed success=true marks step as done", () => {
    const { container } = render(
      <ProcedureRunDagPanel
        procedureName="p"
        steps={[{ name: "step_a", task_type: "execute_sql" }]}
        events={[stepStarted("step_a"), stepCompleted("step_a", true)]}
        isRunning={false}
        onClose={noop}
      />
    );
    const content = container.querySelector("[data-slot='panel-content']") as HTMLElement;
    expect(within(content).getByText("Done")).toBeInTheDocument();
    expect(within(content).queryByText("Running…")).toBeNull();
  });

  it("procedure_step_completed success=false marks step as failed", () => {
    const { container } = render(
      <ProcedureRunDagPanel
        procedureName="p"
        steps={[{ name: "step_a", task_type: "execute_sql" }]}
        events={[stepStarted("step_a"), stepCompleted("step_a", false, "connection refused")]}
        isRunning={false}
        onClose={noop}
      />
    );
    const content = container.querySelector("[data-slot='panel-content']") as HTMLElement;
    expect(within(content).getByText("Failed")).toBeInTheDocument();
    expect(within(content).queryByText("Running…")).toBeNull();
    expect(within(content).queryByText("Done")).toBeNull();
  });

  it("tracks multiple steps independently", () => {
    const { container } = render(
      <ProcedureRunDagPanel
        procedureName="p"
        steps={[
          { name: "step_a", task_type: "execute_sql" },
          { name: "step_b", task_type: "execute_sql" },
          { name: "step_c", task_type: "execute_sql" }
        ]}
        events={[
          stepStarted("step_a"),
          stepCompleted("step_a", true),
          stepStarted("step_b")
        ]}
        isRunning={false}
        onClose={noop}
      />
    );
    const content = container.querySelector("[data-slot='panel-content']") as HTMLElement;
    // step_a → done, step_b → running, step_c → idle
    expect(within(content).queryAllByText("Done")).toHaveLength(1);
    expect(within(content).queryAllByText("Running…")).toHaveLength(1);
    expect(within(content).queryAllByText("Failed")).toHaveLength(0);
  });

  it("step absent from steps list does not affect rendering", () => {
    // An unknown step name in events should not crash or affect the known steps.
    const { container } = render(
      <ProcedureRunDagPanel
        procedureName="p"
        steps={[{ name: "step_a", task_type: "execute_sql" }]}
        events={[stepStarted("unknown_step")]}
        isRunning={false}
        onClose={noop}
      />
    );
    const content = container.querySelector("[data-slot='panel-content']") as HTMLElement;
    // step_a is still idle; unknown_step is not rendered
    expect(within(content).queryByText("Running…")).toBeNull();
    expect(screen.getByText("step_a")).toBeInTheDocument();
  });

  it("last event wins when a step has duplicate started events", () => {
    // procedure_step_started fired twice for the same step, then completed
    const { container } = render(
      <ProcedureRunDagPanel
        procedureName="p"
        steps={[{ name: "step_a", task_type: "execute_sql" }]}
        events={[
          stepStarted("step_a"),
          stepStarted("step_a"), // duplicate
          stepCompleted("step_a", true)
        ]}
        isRunning={false}
        onClose={noop}
      />
    );
    const content = container.querySelector("[data-slot='panel-content']") as HTMLElement;
    expect(within(content).getByText("Done")).toBeInTheDocument();
  });
});

// ── Incremental rerender (streaming simulation) ───────────────────────────────

describe("ProcedureRunDagPanel — incremental rerender", () => {
  it("step transitions: idle → running → done across rerenders", () => {
    const allEvents = [
      stepStarted("step_a"),
      stepCompleted("step_a", true),
      procedureCompleted("p", true)
    ];
    const props = (n: number, running = true) => ({
      procedureName: "p",
      steps: [{ name: "step_a", task_type: "execute_sql" }],
      events: allEvents.slice(0, n),
      isRunning: running,
      onClose: noop
    });

    const { rerender, container } = render(<ProcedureRunDagPanel {...props(0)} />);
    const content = container.querySelector("[data-slot='panel-content']") as HTMLElement;

    // 1) idle — no status label
    expect(within(content).queryByText("Running…")).toBeNull();
    expect(within(content).queryByText("Done")).toBeNull();

    // 2) step_started — running
    rerender(<ProcedureRunDagPanel {...props(1, true)} />);
    expect(within(content).getByText("Running…")).toBeInTheDocument();

    // 3) step_completed — done
    rerender(<ProcedureRunDagPanel {...props(2, false)} />);
    expect(within(content).getByText("Done")).toBeInTheDocument();
    expect(within(content).queryByText("Running…")).toBeNull();
  });

  it("header subtitle transitions: Running → Completed across rerenders", () => {
    const allEvents = [
      stepStarted("step_a"),
      stepCompleted("step_a", true),
      procedureCompleted("p", true)
    ];

    const { rerender } = render(
      <ProcedureRunDagPanel
        procedureName="p"
        steps={[{ name: "step_a", task_type: "execute_sql" }]}
        events={allEvents.slice(0, 1)}
        isRunning={true}
        onClose={noop}
      />
    );
    const header = screen
      .getByRole("heading", { name: "p" })
      .closest("[data-slot='panel-header']") as HTMLElement;

    expect(within(header).getByText("Running…")).toBeInTheDocument();

    // Procedure completed
    rerender(
      <ProcedureRunDagPanel
        procedureName="p"
        steps={[{ name: "step_a", task_type: "execute_sql" }]}
        events={allEvents}
        isRunning={false}
        onClose={noop}
      />
    );
    expect(within(header).getByText("Completed")).toBeInTheDocument();
    expect(within(header).queryByText("Running…")).toBeNull();
  });

  it("header subtitle transitions: Running → Failed across rerenders", () => {
    const allEvents = [
      stepStarted("step_a"),
      stepCompleted("step_a", false, "query error"),
      procedureCompleted("p", false, "query error")
    ];

    const { rerender } = render(
      <ProcedureRunDagPanel
        procedureName="p"
        steps={[{ name: "step_a", task_type: "execute_sql" }]}
        events={allEvents.slice(0, 1)}
        isRunning={true}
        onClose={noop}
      />
    );
    const header = screen
      .getByRole("heading", { name: "p" })
      .closest("[data-slot='panel-header']") as HTMLElement;

    rerender(
      <ProcedureRunDagPanel
        procedureName="p"
        steps={[{ name: "step_a", task_type: "execute_sql" }]}
        events={allEvents}
        isRunning={false}
        onClose={noop}
      />
    );
    expect(within(header).getByText("Failed")).toBeInTheDocument();
  });

  it("most recent procedure_completed is used when duplicates exist", () => {
    // If procedure_completed fires more than once, the last one wins
    const { container } = render(
      <ProcedureRunDagPanel
        procedureName="p"
        steps={[{ name: "step_a", task_type: "execute_sql" }]}
        events={[
          procedureCompleted("p", false, "first failure"),
          procedureCompleted("p", true) // second, successful retry
        ]}
        isRunning={false}
        onClose={noop}
      />
    );
    const header = container.querySelector("[data-slot='panel-header']") as HTMLElement;
    expect(within(header).getByText("Completed")).toBeInTheDocument();
    expect(within(header).queryByText("Failed")).toBeNull();
  });
});

// ── Close button ──────────────────────────────────────────────────────────────

describe("ProcedureRunDagPanel — close button", () => {
  it("calls onClose when the X button is clicked", () => {
    const onClose = vi.fn();
    render(
      <ProcedureRunDagPanel
        procedureName="p"
        steps={STEPS}
        events={[]}
        isRunning={false}
        onClose={onClose}
      />
    );
    fireEvent.click(screen.getByRole("button", { name: "Close panel" }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("does not call onClose when clicking elsewhere", () => {
    const onClose = vi.fn();
    render(
      <ProcedureRunDagPanel
        procedureName="p"
        steps={STEPS}
        events={[]}
        isRunning={false}
        onClose={onClose}
      />
    );
    fireEvent.click(screen.getByText(STEPS[0].name));
    expect(onClose).not.toHaveBeenCalled();
  });
});
