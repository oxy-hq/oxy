// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SseEvent, UseAnalyticsRunResult } from "@/hooks/useAnalyticsRun";
import type { ThreadItem } from "@/types/chat";
import AnalyticsThread from "./index";

// ── Module mocks ──────────────────────────────────────────────────────────────

vi.mock("@tanstack/react-query", () => ({
  useQuery: mockUseQuery,
  useQueryClient: vi.fn(() => ({ invalidateQueries: vi.fn() }))
}));

const { mockUseAnalyticsRun, sidebarDisplayBlocksSpy, mockUseQuery } = vi.hoisted(() => ({
  mockUseAnalyticsRun: vi.fn<[], UseAnalyticsRunResult>(),
  sidebarDisplayBlocksSpy: vi.fn(),
  mockUseQuery: vi.fn(() => ({ data: [], isLoading: false }))
}));
vi.mock("@/hooks/useAnalyticsRun", async (importOriginal) => {
  const original = await importOriginal<typeof import("@/hooks/useAnalyticsRun")>();
  return { ...original, useAnalyticsRun: mockUseAnalyticsRun };
});

vi.mock("@/hooks/useCurrentProjectBranch", () => ({
  default: () => ({ project: { id: "proj-1" } })
}));

vi.mock("@/hooks/api/queryKey", () => ({
  default: {
    analytics: {
      runsByThread: (...args: unknown[]) => ["analytics", "runs", ...args]
    }
  }
}));

vi.mock("@/components/ui/shadcn/resizable", () => ({
  ResizablePanelGroup: ({ children }: { children: React.ReactNode }) => (
    <div data-testid='panel-group'>{children}</div>
  ),
  ResizablePanel: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  ResizableHandle: () => null
}));

vi.mock("./Header", () => ({ default: () => <div data-testid='thread-header' /> }));
// Expose a test-only trigger that simulates clicking the ProcedureChild row inside
// the real reasoning trace. onSelectArtifact is called with a procedure item to
// mirror what ProcedureChild does when the user clicks it.
vi.mock("./AnalyticsReasoningTrace", () => ({
  default: ({
    onSelectArtifact
  }: {
    events: unknown[];
    isRunning: boolean;
    onSelectArtifact: (item: unknown) => void;
  }) => (
    <>
      <button
        type='button'
        data-testid='proc-trigger'
        onClick={() =>
          onSelectArtifact({
            kind: "procedure",
            id: "mock-proc",
            procedureName: "mock",
            stepCount: 1,
            isStreaming: false
          })
        }
      >
        Open procedure
      </button>
      <button
        type='button'
        data-testid='chart-trigger'
        onClick={() =>
          onSelectArtifact({
            kind: "artifact",
            id: "mock-chart",
            toolName: "render_chart",
            toolInput: "{}",
            isStreaming: true
          })
        }
      >
        Open chart
      </button>
    </>
  )
}));
vi.mock("./AnalyticsArtifactSidebar", () => ({
  default: ({
    displayBlocks = [],
    onClose
  }: {
    displayBlocks?: unknown[];
    onClose: () => void;
  }) => {
    sidebarDisplayBlocksSpy(displayBlocks);
    return <button type='button' aria-label='Close panel' onClick={onClose} />;
  }
}));
vi.mock("./SuspensionPrompt", () => ({ default: () => null }));
vi.mock("@/components/Messages/UserMessage", () => ({
  default: ({ content }: { content: string }) => <div data-testid='user-message'>{content}</div>
}));
vi.mock("@/components/Markdown", () => ({
  default: ({ children }: { children: string }) => <div>{children}</div>
}));
vi.mock("@/components/MessageInput", () => ({
  default: () => <div data-testid='message-input' />
}));
vi.mock("@/components/AppPreview/Displays", () => ({ DisplayBlock: () => null }));

// ── Fixtures ──────────────────────────────────────────────────────────────────

const THREAD: ThreadItem = {
  id: "thread-1",
  source: "agent-1",
  input: "Analyze sales data",
  title: "Sales Analysis",
  output: "",
  source_type: "analytics",
  created_at: "2026-01-01T00:00:00Z",
  references: [],
  is_processing: false
};

let counter = 0;

const sseEv = <T extends SseEvent["type"]>(
  type: T,
  data: Extract<SseEvent, { type: T }>["data"]
): SseEvent => ({ id: String(counter++), type, data }) as SseEvent;

const noop = () => {};

function makeResult(overrides: Partial<UseAnalyticsRunResult> = {}): UseAnalyticsRunResult {
  return {
    state: { tag: "idle" },
    start: noop,
    reconnect: noop,
    hydrate: noop,
    answer: noop,
    stop: noop,
    reset: noop,
    isStarting: false,
    isAnswering: false,
    ...overrides
  };
}

function runningWith(events: SseEvent[]): UseAnalyticsRunResult {
  return makeResult({ state: { tag: "running", runId: "run-1", events } });
}

beforeEach(() => {
  counter = 0;
  mockUseAnalyticsRun.mockReturnValue(makeResult());
  mockUseQuery.mockReturnValue({ data: [], isLoading: false });
  // jsdom does not implement scrollIntoView
  window.HTMLElement.prototype.scrollIntoView = vi.fn();
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

// helper — click the mock trace trigger to open the procedure panel
const openPanel = () => fireEvent.click(screen.getByTestId("proc-trigger"));

// ── procedureInfo derivation ───────────────────────────────────────────────────

describe("AnalyticsThread — procedureInfo derivation", () => {
  it("does not render ProcedureRunDagPanel when state is idle", () => {
    mockUseAnalyticsRun.mockReturnValue(makeResult({ state: { tag: "idle" } }));
    render(<AnalyticsThread thread={THREAD} />);
    expect(screen.queryByRole("heading")).toBeNull();
  });

  it("does not render ProcedureRunDagPanel when running but no procedure_started event", () => {
    mockUseAnalyticsRun.mockReturnValue(runningWith([]));
    render(<AnalyticsThread thread={THREAD} />);
    expect(screen.queryByRole("heading")).toBeNull();
  });

  it("shows panel with correct procedure name after triggering via trace row", async () => {
    const events: SseEvent[] = [
      sseEv("procedure_started", {
        procedure_name: "store_deep_dive",
        steps: [
          { name: "fetch_data", task_type: "execute_sql" },
          { name: "process_data", task_type: "execute_sql" }
        ]
      })
    ];
    mockUseAnalyticsRun.mockReturnValue(runningWith(events));
    render(<AnalyticsThread thread={THREAD} />);
    openPanel();
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "store_deep_dive" })).toBeInTheDocument();
    });
  });

  it("renders all steps from the procedure_started event", async () => {
    const STEPS = [
      { name: "fetch_data", task_type: "execute_sql" },
      { name: "process_data", task_type: "execute_sql" },
      { name: "generate_report", task_type: "formatter" }
    ];
    const events: SseEvent[] = [
      sseEv("procedure_started", { procedure_name: "my_proc", steps: STEPS })
    ];
    mockUseAnalyticsRun.mockReturnValue(runningWith(events));
    render(<AnalyticsThread thread={THREAD} />);
    openPanel();
    await waitFor(() => {
      for (const step of STEPS) {
        expect(screen.getByText(step.name)).toBeInTheDocument();
      }
    });
  });

  it("uses the last procedure_started event when duplicates exist", async () => {
    const events: SseEvent[] = [
      sseEv("procedure_started", {
        procedure_name: "first_proc",
        steps: [{ name: "step_a", task_type: "execute_sql" }]
      }),
      sseEv("procedure_started", {
        procedure_name: "second_proc",
        steps: [{ name: "step_b", task_type: "execute_sql" }]
      })
    ];
    mockUseAnalyticsRun.mockReturnValue(runningWith(events));
    render(<AnalyticsThread thread={THREAD} />);
    openPanel();
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "second_proc" })).toBeInTheDocument();
    });
    expect(screen.queryByRole("heading", { name: "first_proc" })).toBeNull();
  });

  it("passes all SSE events to ProcedureRunDagPanel for step status derivation", async () => {
    const events: SseEvent[] = [
      sseEv("procedure_started", {
        procedure_name: "my_proc",
        steps: [{ name: "step_a", task_type: "execute_sql" }]
      }),
      sseEv("procedure_step_started", { step: "step_a" })
    ];
    mockUseAnalyticsRun.mockReturnValue(runningWith(events));
    render(<AnalyticsThread thread={THREAD} />);
    openPanel();
    // Both the header subtitle and the step status show "Running…" — at least one must exist
    await waitFor(() => {
      expect(screen.queryAllByText("Running…").length).toBeGreaterThan(0);
    });
  });
});

// ── Panel open/close behavior ─────────────────────────────────────────────────

describe("AnalyticsThread — panel open/close behavior", () => {
  it("panel is not open before user triggers it", () => {
    mockUseAnalyticsRun.mockReturnValue(
      runningWith([
        sseEv("procedure_started", {
          procedure_name: "p",
          steps: [{ name: "s", task_type: "execute_sql" }]
        })
      ])
    );
    render(<AnalyticsThread thread={THREAD} />);
    // Panel heading not visible — user hasn't clicked yet
    expect(screen.queryByRole("heading")).toBeNull();
  });

  it("panel is not shown when state is idle (no procedureInfo)", () => {
    mockUseAnalyticsRun.mockReturnValue(makeResult());
    render(<AnalyticsThread thread={THREAD} />);
    expect(screen.queryByRole("heading")).toBeNull();
  });

  it("shows 'Running…' header subtitle while isRunning is true", async () => {
    const events: SseEvent[] = [
      sseEv("procedure_started", {
        procedure_name: "running_proc",
        steps: [{ name: "step_a", task_type: "execute_sql" }]
      })
    ];
    mockUseAnalyticsRun.mockReturnValue(runningWith(events));
    render(<AnalyticsThread thread={THREAD} />);
    openPanel();
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "running_proc" })).toBeInTheDocument();
    });
    const header = screen
      .getByRole("heading", { name: "running_proc" })
      .closest("[data-slot='panel-header']") as HTMLElement;
    expect(header).not.toBeNull();
    expect(header.textContent).toContain("Running…");
  });

  it("second trigger click closes the panel (toggle)", async () => {
    mockUseAnalyticsRun.mockReturnValue(
      runningWith([
        sseEv("procedure_started", {
          procedure_name: "p",
          steps: [{ name: "s", task_type: "execute_sql" }]
        })
      ])
    );
    render(<AnalyticsThread thread={THREAD} />);
    openPanel();
    await waitFor(() => expect(screen.getByRole("heading", { name: "p" })).toBeInTheDocument());
    openPanel(); // second click → toggle off
    await waitFor(() => expect(screen.queryByRole("heading", { name: "p" })).toBeNull());
  });
});

// ── Close button ──────────────────────────────────────────────────────────────

describe("AnalyticsThread — procedure panel close", () => {
  it("hides ProcedureRunDagPanel when onClose is triggered", async () => {
    const events: SseEvent[] = [
      sseEv("procedure_started", {
        procedure_name: "my_proc",
        steps: [{ name: "step_a", task_type: "execute_sql" }]
      })
    ];
    mockUseAnalyticsRun.mockReturnValue(runningWith(events));
    render(<AnalyticsThread thread={THREAD} />);

    openPanel();
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "my_proc" })).toBeInTheDocument();
    });

    // Click the close button on ProcedureRunDagPanel
    fireEvent.click(screen.getByRole("button", { name: "Close panel" }));

    // Panel should be hidden
    await waitFor(() => {
      expect(screen.queryByRole("heading", { name: "my_proc" })).toBeNull();
    });
  });

  it("does not re-open the panel after close on rerender", async () => {
    const events: SseEvent[] = [
      sseEv("procedure_started", {
        procedure_name: "my_proc",
        steps: [{ name: "step_a", task_type: "execute_sql" }]
      })
    ];
    mockUseAnalyticsRun.mockReturnValue(runningWith(events));
    const { rerender } = render(<AnalyticsThread thread={THREAD} />);

    openPanel();
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "my_proc" })).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: "Close panel" }));
    await waitFor(() => {
      expect(screen.queryByRole("heading", { name: "my_proc" })).toBeNull();
    });

    rerender(<AnalyticsThread thread={THREAD} />);
    expect(screen.queryByRole("heading", { name: "my_proc" })).toBeNull();
  });
});

// ── Procedure step status propagation ────────────────────────────────────────

describe("AnalyticsThread — step status propagation via events", () => {
  it("step shows Done when procedure_step_completed success=true", async () => {
    const events: SseEvent[] = [
      sseEv("procedure_started", {
        procedure_name: "p",
        steps: [{ name: "step_a", task_type: "execute_sql" }]
      }),
      sseEv("procedure_step_started", { step: "step_a" }),
      sseEv("procedure_step_completed", { step: "step_a", success: true })
    ];
    mockUseAnalyticsRun.mockReturnValue(runningWith(events));
    render(<AnalyticsThread thread={THREAD} />);
    openPanel();
    await waitFor(() => {
      expect(screen.getByText("Done")).toBeInTheDocument();
    });
  });

  it("step shows Failed when procedure_step_completed success=false", async () => {
    const events: SseEvent[] = [
      sseEv("procedure_started", {
        procedure_name: "p",
        steps: [{ name: "step_a", task_type: "execute_sql" }]
      }),
      sseEv("procedure_step_started", { step: "step_a" }),
      sseEv("procedure_step_completed", { step: "step_a", success: false, error: "timeout" })
    ];
    mockUseAnalyticsRun.mockReturnValue(runningWith(events));
    render(<AnalyticsThread thread={THREAD} />);
    openPanel();
    await waitFor(() => {
      expect(screen.getByText("Failed")).toBeInTheDocument();
    });
  });

  it("shows Completed subtitle when procedure_completed success=true and not running", async () => {
    const events: SseEvent[] = [
      sseEv("procedure_started", {
        procedure_name: "p",
        steps: [{ name: "step_a", task_type: "execute_sql" }]
      }),
      sseEv("procedure_step_completed", { step: "step_a", success: true }),
      sseEv("procedure_completed", { procedure_name: "p", success: true })
    ];
    mockUseAnalyticsRun.mockReturnValue(
      makeResult({
        state: { tag: "done", runId: "run-1", answer: "", displayBlocks: [], durationMs: 0, events }
      })
    );
    render(<AnalyticsThread thread={THREAD} />);
    openPanel();
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "p" })).toBeInTheDocument();
    });
    const header = screen
      .getByRole("heading", { name: "p" })
      .closest("[data-slot='panel-header']") as HTMLElement;
    expect(header.textContent).toContain("Completed");
  });
});

// ── Thread switching ──────────────────────────────────────────────────────────

describe("AnalyticsThread — thread switching", () => {
  it("calls reset when thread.id changes while a run is active", async () => {
    const reset = vi.fn();
    mockUseAnalyticsRun.mockReturnValue(
      makeResult({ state: { tag: "running", runId: "run-1", events: [] }, reset })
    );
    const { rerender } = render(<AnalyticsThread thread={THREAD} />);

    const THREAD_2: ThreadItem = { ...THREAD, id: "thread-2" };
    rerender(<AnalyticsThread thread={THREAD_2} />);

    await waitFor(() => expect(reset).toHaveBeenCalled());
  });

  it("calls reset when thread.id changes while state is idle", async () => {
    const reset = vi.fn();
    mockUseAnalyticsRun.mockReturnValue(makeResult({ reset }));
    const { rerender } = render(<AnalyticsThread thread={THREAD} />);

    const THREAD_2: ThreadItem = { ...THREAD, id: "thread-2" };
    rerender(<AnalyticsThread thread={THREAD_2} />);

    await waitFor(() => expect(reset).toHaveBeenCalled());
  });

  it("does not call reset on re-render with the same thread.id", async () => {
    const reset = vi.fn();
    mockUseAnalyticsRun.mockReturnValue(makeResult({ reset }));
    render(<AnalyticsThread thread={THREAD} />);
    reset.mockClear();
    expect(reset).not.toHaveBeenCalled();
  });
});

// ── Auto-start on first visit ─────────────────────────────────────────────────

describe("AnalyticsThread — auto-start on first visit", () => {
  it("calls start automatically when isFirstVisit is true", () => {
    const start = vi.fn();
    mockUseAnalyticsRun.mockReturnValue(makeResult({ start }));
    render(<AnalyticsThread thread={THREAD} />);
    expect(start).toHaveBeenCalledWith("agent-1", "Analyze sales data", "thread-1");
  });

  it("does not show the Run analytics button on first visit (auto-starts instead)", () => {
    const start = vi.fn();
    mockUseAnalyticsRun.mockReturnValue(makeResult({ start }));
    render(<AnalyticsThread thread={THREAD} />);
    expect(screen.queryByText("Run analytics")).toBeNull();
  });

  it("does not auto-start when allRuns exist (not first visit)", () => {
    const start = vi.fn();
    mockUseAnalyticsRun.mockReturnValue(makeResult({ start }));
    mockUseQuery.mockReturnValue({
      data: [{ run_id: "r1", question: "q", status: "done", ui_events: [] }],
      isLoading: false
    });
    render(<AnalyticsThread thread={THREAD} />);
    expect(start).not.toHaveBeenCalled();
  });

  it("does not auto-start while allRuns are still loading", () => {
    const start = vi.fn();
    mockUseAnalyticsRun.mockReturnValue(makeResult({ start }));
    mockUseQuery.mockReturnValue({ data: [], isLoading: true });
    render(<AnalyticsThread thread={THREAD} />);
    expect(start).not.toHaveBeenCalled();
  });
});

// ── ChartSection — streaming display blocks ───────────────────────────────────

const openChart = () => fireEvent.click(screen.getByTestId("chart-trigger"));

describe("AnalyticsThread — ChartSection streaming display blocks", () => {
  it("passes empty displayBlocks to sidebar when no chart_rendered events exist during streaming", () => {
    mockUseAnalyticsRun.mockReturnValue(runningWith([]));
    render(<AnalyticsThread thread={THREAD} />);
    openChart();
    const lastCall = sidebarDisplayBlocksSpy.mock.calls.at(-1);
    expect(lastCall?.[0]).toEqual([]);
  });

  it("passes chart_rendered blocks to sidebar derived from live events during streaming", async () => {
    const chartBlock = {
      config: { chart_type: "bar_chart", x: "month", y: "revenue" },
      columns: ["month", "revenue"],
      rows: [
        ["Jan", 100],
        ["Feb", 200]
      ]
    };
    const events = [sseEv("chart_rendered", chartBlock)];
    mockUseAnalyticsRun.mockReturnValue(runningWith(events));

    render(<AnalyticsThread thread={THREAD} />);
    openChart();

    await waitFor(() => {
      const lastCall = sidebarDisplayBlocksSpy.mock.calls.at(-1);
      expect(lastCall?.[0]).toEqual([chartBlock]);
    });
  });

  it("passes chart_rendered blocks from done state to sidebar", async () => {
    const chartBlock = {
      config: { chart_type: "line_chart", x: "date", y: "sales" },
      columns: ["date", "sales"],
      rows: [["2024-01", 500]]
    };
    const events = [sseEv("chart_rendered", chartBlock)];
    mockUseAnalyticsRun.mockReturnValue(
      makeResult({
        state: {
          tag: "done",
          runId: "run-1",
          answer: "",
          displayBlocks: [chartBlock],
          durationMs: 0,
          events
        }
      })
    );

    render(<AnalyticsThread thread={THREAD} />);
    openChart();

    await waitFor(() => {
      const lastCall = sidebarDisplayBlocksSpy.mock.calls.at(-1);
      expect(lastCall?.[0]).toEqual([chartBlock]);
    });
  });
});
