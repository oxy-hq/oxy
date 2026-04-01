// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { UseAppBuilderRunResult } from "@/hooks/useAppBuilderRun";
import type { ThreadItem } from "@/types/chat";
import AppBuilderThread from "./index";

// ── Module mocks ──────────────────────────────────────────────────────────────

vi.mock("@tanstack/react-query", () => ({
  useQuery: mockUseQuery,
  useQueryClient: vi.fn(() => ({ invalidateQueries: vi.fn() }))
}));

const { mockUseAppBuilderRun, saveRunMock, mockUseQuery } = vi.hoisted(() => ({
  mockUseAppBuilderRun: vi.fn<[], UseAppBuilderRunResult>(),
  saveRunMock: vi.fn().mockResolvedValue({
    app_path64: "Z2VuZXJhdGVkL3J1bi0xLmFwcC55bWw=",
    app_path: "generated/run-1.app.yml"
  }),
  mockUseQuery: vi.fn(() => ({ data: [], isLoading: false }))
}));

vi.mock("@/hooks/useAppBuilderRun", async (importOriginal) => {
  const original = await importOriginal<typeof import("@/hooks/useAppBuilderRun")>();
  return { ...original, useAppBuilderRun: mockUseAppBuilderRun };
});

vi.mock("@/hooks/useCurrentProjectBranch", () => ({
  default: () => ({ project: { id: "proj-1" } })
}));

vi.mock("@/hooks/api/queryKey", () => ({
  default: {
    appBuilder: {
      runsByThread: (...args: unknown[]) => ["appBuilder", "runs", ...args]
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

vi.mock("./Header", () => ({ default: () => <div data-testid='app-builder-header' /> }));

vi.mock("./AppBuilderReasoningTrace", () => ({
  default: ({
    onSelectArtifact
  }: {
    events: unknown[];
    isRunning: boolean;
    onSelectArtifact: (item: unknown) => void;
  }) => (
    <div data-testid='reasoning-trace'>
      <button
        type='button'
        data-testid='artifact-trigger'
        onClick={() =>
          onSelectArtifact({
            kind: "artifact",
            id: "mock-artifact",
            toolName: "execute_preview",
            toolInput: '{"sql":"SELECT 1"}',
            isStreaming: false
          })
        }
      >
        Open artifact
      </button>
    </div>
  )
}));

vi.mock("./AppBuilderArtifactSidebar", () => ({
  default: ({ item, onClose }: { item: { kind: string }; onClose: () => void }) => {
    if (item.kind === "app_preview") {
      return (
        <div data-testid='app-preview-sidebar'>
          <button type='button' aria-label='Close preview' onClick={onClose} />
        </div>
      );
    }
    return (
      <div data-testid='artifact-sidebar'>
        <button type='button' aria-label='Close artifact' onClick={onClose} />
      </div>
    );
  }
}));

vi.mock("../analytics/SuspensionPrompt", () => ({
  default: ({
    onAnswer
  }: {
    questions: unknown[];
    onAnswer: (t: string) => void;
    isAnswering: boolean;
  }) => (
    <div data-testid='suspension-prompt'>
      <button type='button' onClick={() => onAnswer("my answer")}>
        Submit answer
      </button>
    </div>
  )
}));

vi.mock("@/components/Messages/UserMessage", () => ({
  default: ({ content }: { content: string }) => <div data-testid='user-message'>{content}</div>
}));

vi.mock("@/components/MessageInput", () => ({
  default: ({
    onSend,
    disabled
  }: {
    onSend: () => void;
    onChange: (v: string) => void;
    value: string;
    onStop: () => void;
    disabled: boolean;
    isLoading: boolean;
  }) => (
    <button type='button' data-testid='message-input' onClick={onSend} disabled={disabled}>
      Send
    </button>
  )
}));

// Mock lottie to avoid canvas dependency in jsdom
vi.mock("@lottiefiles/react-lottie-player", () => ({
  Player: () => null
}));

vi.mock("@/services/api/appBuilder", () => ({
  AppBuilderService: {
    getRunsByThread: vi.fn().mockResolvedValue([]),
    saveRun: saveRunMock
  }
}));

// ── Fixtures ──────────────────────────────────────────────────────────────────

const THREAD: ThreadItem = {
  id: "thread-1",
  source: "agent-1",
  input: "Build a revenue dashboard",
  title: "Revenue Dashboard",
  output: "",
  source_type: "app_builder",
  created_at: "2026-01-01T00:00:00Z",
  references: [],
  is_processing: false
};

const noop = () => {};

function makeResult(overrides: Partial<UseAppBuilderRunResult> = {}): UseAppBuilderRunResult {
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

beforeEach(() => {
  mockUseAppBuilderRun.mockReturnValue(makeResult());
  mockUseQuery.mockReturnValue({ data: [], isLoading: false });
  window.HTMLElement.prototype.scrollIntoView = vi.fn();
  saveRunMock.mockResolvedValue({
    app_path64: "Z2VuZXJhdGVkL3J1bi0xLmFwcC55bWw=",
    app_path: "generated/run-1.app.yml"
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("AppBuilderThread — initial render", () => {
  it("renders header and message input on empty thread", () => {
    render(<AppBuilderThread thread={THREAD} />);
    expect(screen.getByTestId("app-builder-header")).toBeInTheDocument();
    expect(screen.getByTestId("message-input")).toBeInTheDocument();
  });

  it("shows the thread input as user message on first visit", () => {
    render(<AppBuilderThread thread={THREAD} />);
    expect(screen.getByText("Build a revenue dashboard")).toBeInTheDocument();
  });

  it("auto-starts on first visit instead of showing a button", () => {
    const start = vi.fn();
    mockUseAppBuilderRun.mockReturnValue(makeResult({ start }));
    render(<AppBuilderThread thread={THREAD} />);
    expect(screen.queryByText("Build app")).toBeNull();
    expect(start).toHaveBeenCalled();
  });

  it("does not render sidebar initially", () => {
    render(<AppBuilderThread thread={THREAD} />);
    expect(screen.queryByTestId("app-preview-sidebar")).toBeNull();
  });
});

describe("AppBuilderThread — running state", () => {
  it("shows reasoning trace while running", () => {
    mockUseAppBuilderRun.mockReturnValue(
      makeResult({ state: { tag: "running", runId: "run-1", events: [] } })
    );
    render(<AppBuilderThread thread={THREAD} />);
    expect(screen.getByTestId("reasoning-trace")).toBeInTheDocument();
  });

  it("does not show sidebar while running", () => {
    mockUseAppBuilderRun.mockReturnValue(
      makeResult({ state: { tag: "running", runId: "run-1", events: [] } })
    );
    render(<AppBuilderThread thread={THREAD} />);
    expect(screen.queryByTestId("app-preview-sidebar")).toBeNull();
  });

  it("disables message input while running", () => {
    mockUseAppBuilderRun.mockReturnValue(
      makeResult({ state: { tag: "running", runId: "run-1", events: [] } })
    );
    render(<AppBuilderThread thread={THREAD} />);
    expect(screen.getByTestId("message-input")).toBeDisabled();
  });
});

describe("AppBuilderThread — suspended state", () => {
  it("shows suspension prompt when suspended", () => {
    mockUseAppBuilderRun.mockReturnValue(
      makeResult({
        state: {
          tag: "suspended",
          runId: "run-1",
          events: [],
          questions: [{ prompt: "Which connector?", suggestions: [] }]
        }
      })
    );
    render(<AppBuilderThread thread={THREAD} />);
    expect(screen.getByTestId("suspension-prompt")).toBeInTheDocument();
  });

  it("calls answer() when suspension prompt is submitted", () => {
    const answer = vi.fn();
    mockUseAppBuilderRun.mockReturnValue(
      makeResult({
        state: {
          tag: "suspended",
          runId: "run-1",
          events: [],
          questions: [{ prompt: "Which connector?", suggestions: [] }]
        },
        answer
      })
    );
    render(<AppBuilderThread thread={THREAD} />);
    fireEvent.click(screen.getByText("Submit answer"));
    expect(answer).toHaveBeenCalledWith("my answer");
  });
});

describe("AppBuilderThread — done state", () => {
  it("calls saveRun when run transitions to done", async () => {
    mockUseAppBuilderRun.mockReturnValue(
      makeResult({ state: { tag: "done", runId: "run-1", durationMs: 3000, events: [] } })
    );
    render(<AppBuilderThread thread={THREAD} />);
    await waitFor(() => {
      expect(saveRunMock).toHaveBeenCalledWith("proj-1", "run-1");
    });
  });

  it("opens app preview sidebar after save completes", async () => {
    mockUseAppBuilderRun.mockReturnValue(
      makeResult({ state: { tag: "done", runId: "run-1", durationMs: 3000, events: [] } })
    );
    render(<AppBuilderThread thread={THREAD} />);
    await waitFor(() => {
      expect(screen.getByTestId("app-preview-sidebar")).toBeInTheDocument();
    });
  });

  it("shows success message when done", () => {
    // saveRun won't resolve immediately so sidebar isn't open yet
    saveRunMock.mockReturnValue(new Promise(() => {}));
    mockUseAppBuilderRun.mockReturnValue(
      makeResult({ state: { tag: "done", runId: "run-1", durationMs: 3000, events: [] } })
    );
    render(<AppBuilderThread thread={THREAD} />);
    expect(screen.getByText("App built successfully.")).toBeInTheDocument();
  });

  it("closing sidebar sets userClosedSidebarRef so it does not reopen", async () => {
    mockUseAppBuilderRun.mockReturnValue(
      makeResult({ state: { tag: "done", runId: "run-1", durationMs: 3000, events: [] } })
    );
    render(<AppBuilderThread thread={THREAD} />);
    await waitFor(() => {
      expect(screen.getByTestId("app-preview-sidebar")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByLabelText("Close preview"));
    await waitFor(() => {
      expect(screen.queryByTestId("app-preview-sidebar")).toBeNull();
    });
  });
});

describe("AppBuilderThread — failed state", () => {
  it("shows error message when failed", () => {
    mockUseAppBuilderRun.mockReturnValue(
      makeResult({
        state: {
          tag: "failed",
          runId: "run-1",
          message: "SQL execution failed",
          durationMs: 0,
          events: []
        }
      })
    );
    render(<AppBuilderThread thread={THREAD} />);
    expect(screen.getByText("Build failed")).toBeInTheDocument();
    expect(screen.getByText("SQL execution failed")).toBeInTheDocument();
  });

  it("retry button calls reset and start", async () => {
    const reset = vi.fn();
    const start = vi.fn();
    mockUseAppBuilderRun.mockReturnValue(
      makeResult({
        state: {
          tag: "failed",
          runId: "run-1",
          message: "oops",
          durationMs: 0,
          events: []
        },
        reset,
        start
      })
    );
    render(<AppBuilderThread thread={THREAD} />);
    fireEvent.click(screen.getByText("Retry"));
    expect(reset).toHaveBeenCalled();
    expect(start).toHaveBeenCalled();
  });
});

describe("AppBuilderThread — artifact sidebar", () => {
  it("opens artifact sidebar when an artifact is selected from reasoning trace", () => {
    mockUseAppBuilderRun.mockReturnValue(
      makeResult({ state: { tag: "running", runId: "run-1", events: [] } })
    );
    render(<AppBuilderThread thread={THREAD} />);
    fireEvent.click(screen.getByTestId("artifact-trigger"));
    expect(screen.getByTestId("artifact-sidebar")).toBeInTheDocument();
  });

  it("closes artifact sidebar when close button is clicked", () => {
    mockUseAppBuilderRun.mockReturnValue(
      makeResult({ state: { tag: "running", runId: "run-1", events: [] } })
    );
    render(<AppBuilderThread thread={THREAD} />);
    fireEvent.click(screen.getByTestId("artifact-trigger"));
    expect(screen.getByTestId("artifact-sidebar")).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("Close artifact"));
    expect(screen.queryByTestId("artifact-sidebar")).toBeNull();
  });

  it("artifact sidebar takes priority over app preview sidebar", async () => {
    // Start in done state so the app preview opens
    mockUseAppBuilderRun.mockReturnValue(
      makeResult({ state: { tag: "done", runId: "run-1", durationMs: 3000, events: [] } })
    );
    const { rerender } = render(<AppBuilderThread thread={THREAD} />);
    await waitFor(() => {
      expect(screen.getByTestId("app-preview-sidebar")).toBeInTheDocument();
    });
    // Re-render with running state so the reasoning trace (with trigger) appears
    mockUseAppBuilderRun.mockReturnValue(
      makeResult({ state: { tag: "running", runId: "run-2", events: [] } })
    );
    rerender(<AppBuilderThread thread={THREAD} />);
    // Click artifact trigger — artifact sidebar should replace app preview
    fireEvent.click(screen.getByTestId("artifact-trigger"));
    expect(screen.getByTestId("artifact-sidebar")).toBeInTheDocument();
    expect(screen.queryByTestId("app-preview-sidebar")).toBeNull();
  });
});

// ── Auto-start on first visit ─────────────────────────────────────────────────

describe("AppBuilderThread — auto-start on first visit", () => {
  it("calls start automatically when isFirstVisit is true", () => {
    const start = vi.fn();
    mockUseAppBuilderRun.mockReturnValue(makeResult({ start }));
    render(<AppBuilderThread thread={THREAD} />);
    expect(start).toHaveBeenCalledWith("agent-1", "Build a revenue dashboard", "thread-1");
  });

  it("does not show the Build app button on first visit (auto-starts instead)", () => {
    const start = vi.fn();
    mockUseAppBuilderRun.mockReturnValue(makeResult({ start }));
    render(<AppBuilderThread thread={THREAD} />);
    expect(screen.queryByText("Build app")).toBeNull();
  });

  it("does not auto-start when allRuns exist (not first visit)", () => {
    const start = vi.fn();
    mockUseAppBuilderRun.mockReturnValue(makeResult({ start }));
    mockUseQuery.mockReturnValue({
      data: [{ run_id: "r1", request: "q", status: "done", ui_events: [] }],
      isLoading: false
    });
    render(<AppBuilderThread thread={THREAD} />);
    expect(start).not.toHaveBeenCalled();
  });

  it("does not auto-start while allRuns are still loading", () => {
    const start = vi.fn();
    mockUseAppBuilderRun.mockReturnValue(makeResult({ start }));
    mockUseQuery.mockReturnValue({ data: [], isLoading: true });
    render(<AppBuilderThread thread={THREAD} />);
    expect(start).not.toHaveBeenCalled();
  });
});
