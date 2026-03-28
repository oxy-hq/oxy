// @vitest-environment jsdom

import { renderHook, act, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useAppBuilderRun } from "./useAppBuilderRun";

// ── Module mocks ──────────────────────────────────────────────────────────────

let capturedOptions: {
  onmessage?: (ev: { event: string; data: string; id?: string }) => void;
  onerror?: (err: unknown) => void;
  signal?: AbortSignal;
} = {};

vi.mock("@microsoft/fetch-event-source", () => ({
  fetchEventSource: vi.fn((_url: string, options: typeof capturedOptions) => {
    capturedOptions = options;
    return new Promise<void>(() => {
      // never resolves — simulates a live SSE connection
    });
  })
}));

vi.mock("@/services/api/appBuilder", () => ({
  AppBuilderService: {
    eventsUrl: (_projectId: string, runId: string) =>
      `/api/projects/proj-1/app-builder/app-runs/${runId}/events`,
    createRun: vi.fn().mockResolvedValue({ run_id: "run-1" }),
    cancelRun: vi.fn().mockResolvedValue(undefined),
    submitAnswer: vi.fn().mockResolvedValue(undefined),
    saveRun: vi.fn().mockResolvedValue({ app_path64: "Z2VuZXJhdGVkL3J1bi0xLmFwcC55bWw=", app_path: "generated/run-1.app.yml" })
  }
}));

// ── Helpers ───────────────────────────────────────────────────────────────────

function sendEvent(event: string, data: Record<string, unknown> = {}, id = "") {
  capturedOptions.onmessage?.({ event, data: JSON.stringify(data), id });
}

// ── Setup ─────────────────────────────────────────────────────────────────────

beforeEach(() => {
  capturedOptions = {};
  localStorage.setItem("auth_token", "test-token");
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("useAppBuilderRun — state transitions", () => {
  it("starts as idle", () => {
    const { result } = renderHook(() => useAppBuilderRun({ projectId: "proj-1" }));
    expect(result.current.state.tag).toBe("idle");
  });

  it("transitions idle → running on reconnect()", () => {
    const { result } = renderHook(() => useAppBuilderRun({ projectId: "proj-1" }));
    act(() => {
      result.current.reconnect("run-1");
    });
    expect(result.current.state.tag).toBe("running");
    expect((result.current.state as { tag: "running"; runId: string }).runId).toBe("run-1");
  });

  it("accumulates events while running", () => {
    const { result } = renderHook(() => useAppBuilderRun({ projectId: "proj-1" }));
    act(() => {
      result.current.reconnect("run-1");
    });
    act(() => {
      sendEvent("step_start", { label: "Clarifying" });
      sendEvent("text_delta", { token: "Hello" });
    });
    const state = result.current.state as { tag: "running"; events: { type: string }[] };
    expect(state.events.length).toBe(2);
    expect(state.events[0].type).toBe("step_start");
    expect(state.events[1].type).toBe("text_delta");
  });

  it("transitions running → suspended on awaiting_input", () => {
    const { result } = renderHook(() => useAppBuilderRun({ projectId: "proj-1" }));
    act(() => {
      result.current.reconnect("run-1");
    });
    act(() => {
      sendEvent("awaiting_input", {
        questions: [{ prompt: "Which connector?", suggestions: ["postgres", "bigquery"] }]
      });
    });
    expect(result.current.state.tag).toBe("suspended");
    const s = result.current.state as { tag: "suspended"; questions: { prompt: string }[] };
    expect(s.questions[0].prompt).toBe("Which connector?");
  });

  it("transitions suspended → running on human_input_resolved", () => {
    const { result } = renderHook(() => useAppBuilderRun({ projectId: "proj-1" }));
    act(() => {
      result.current.reconnect("run-1");
    });
    act(() => {
      sendEvent("awaiting_input", { questions: [{ prompt: "Q?", suggestions: [] }] });
    });
    expect(result.current.state.tag).toBe("suspended");
    act(() => {
      sendEvent("human_input_resolved", {});
    });
    expect(result.current.state.tag).toBe("running");
  });

  it("transitions to done on done event with duration_ms", () => {
    const { result } = renderHook(() => useAppBuilderRun({ projectId: "proj-1" }));
    act(() => {
      result.current.reconnect("run-1");
    });
    act(() => {
      sendEvent("done", { duration_ms: 4200 });
    });
    expect(result.current.state.tag).toBe("done");
    const s = result.current.state as { tag: "done"; durationMs: number; runId: string };
    expect(s.durationMs).toBe(4200);
    expect(s.runId).toBe("run-1");
  });

  it("stores all accumulated events in done state", () => {
    const { result } = renderHook(() => useAppBuilderRun({ projectId: "proj-1" }));
    act(() => {
      result.current.reconnect("run-1");
    });
    act(() => {
      sendEvent("step_start", { label: "Clarifying" });
      sendEvent("task_plan_ready", { task_count: 3, control_count: 1 });
      sendEvent("done", { duration_ms: 1000 });
    });
    const s = result.current.state as { tag: "done"; events: { type: string }[] };
    expect(s.events.length).toBe(3);
    expect(s.events[1].type).toBe("task_plan_ready");
  });

  it("transitions to failed on error event", () => {
    const { result } = renderHook(() => useAppBuilderRun({ projectId: "proj-1" }));
    act(() => {
      result.current.reconnect("run-1");
    });
    act(() => {
      sendEvent("error", { message: "SQL execution failed" });
    });
    expect(result.current.state.tag).toBe("failed");
    const s = result.current.state as { tag: "failed"; message: string };
    expect(s.message).toBe("SQL execution failed");
  });

  it("app-builder domain events (task_plan_ready, task_sql_resolved, app_yaml_ready) are stored", () => {
    const { result } = renderHook(() => useAppBuilderRun({ projectId: "proj-1" }));
    act(() => {
      result.current.reconnect("run-1");
    });
    act(() => {
      sendEvent("task_plan_ready", { task_count: 2, control_count: 0 });
      sendEvent("task_sql_resolved", { task_name: "revenue_by_month" });
      sendEvent("app_yaml_ready", { char_count: 512 });
    });
    const s = result.current.state as { tag: "running"; events: { type: string }[] };
    expect(s.events.map((e) => e.type)).toEqual([
      "task_plan_ready",
      "task_sql_resolved",
      "app_yaml_ready"
    ]);
  });
});

describe("useAppBuilderRun — stop / cancel", () => {
  it("transitions to idle and calls cancelRun on stop()", async () => {
    const { AppBuilderService } = await import("@/services/api/appBuilder");
    const { result } = renderHook(() => useAppBuilderRun({ projectId: "proj-1" }));
    act(() => {
      result.current.reconnect("run-1");
    });
    act(() => {
      result.current.stop();
    });
    expect(result.current.state.tag).toBe("idle");
    expect(AppBuilderService.cancelRun).toHaveBeenCalledWith("proj-1", "run-1");
  });

  it("reset() transitions to idle without cancelling", async () => {
    const { AppBuilderService } = await import("@/services/api/appBuilder");
    const { result } = renderHook(() => useAppBuilderRun({ projectId: "proj-1" }));
    act(() => {
      result.current.reconnect("run-1");
    });
    act(() => {
      result.current.reset();
    });
    expect(result.current.state.tag).toBe("idle");
    expect(AppBuilderService.cancelRun).not.toHaveBeenCalled();
  });
});

describe("useAppBuilderRun — onerror behavior", () => {
  it("does not log to console when onerror fires after signal was aborted", async () => {
    const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const { result } = renderHook(() => useAppBuilderRun({ projectId: "proj-1" }));

    act(() => {
      result.current.reconnect("run-1");
    });
    expect(capturedOptions.signal!.aborted).toBe(false);

    act(() => {
      result.current.reset();
    });
    expect(capturedOptions.signal!.aborted).toBe(true);
    expect(result.current.state.tag).toBe("idle");

    const abortErr = new DOMException("The user aborted a request.", "AbortError");
    act(() => {
      try {
        capturedOptions.onerror?.(abortErr);
      } catch {
        // expected re-throw
      }
    });

    expect(result.current.state.tag).toBe("idle");
    expect(consoleSpy).not.toHaveBeenCalled();
    consoleSpy.mockRestore();
  });

  it("logs and sets failed state when onerror fires without an intentional abort", async () => {
    const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const { result } = renderHook(() => useAppBuilderRun({ projectId: "proj-1" }));

    act(() => {
      result.current.reconnect("run-1");
    });

    const networkErr = new Error("Network failure");
    act(() => {
      try {
        capturedOptions.onerror?.(networkErr);
      } catch {
        // expected re-throw
      }
    });

    expect(result.current.state.tag).toBe("failed");
    expect(consoleSpy).toHaveBeenCalledWith("App-builder SSE error:", networkErr);
    consoleSpy.mockRestore();
  });
});

describe("useAppBuilderRun — SSE cleanup on unmount", () => {
  it("aborts the SSE connection when the hook unmounts", () => {
    const { result, unmount } = renderHook(() => useAppBuilderRun({ projectId: "proj-1" }));
    act(() => {
      result.current.reconnect("run-1");
    });
    expect(capturedOptions.signal!.aborted).toBe(false);
    unmount();
    expect(capturedOptions.signal!.aborted).toBe(true);
  });
});

describe("useAppBuilderRun — hydrate", () => {
  it("hydrate with done status sets done state", () => {
    const { result } = renderHook(() => useAppBuilderRun({ projectId: "proj-1" }));
    act(() => {
      result.current.hydrate("run-5", "done", []);
    });
    expect(result.current.state.tag).toBe("done");
    expect((result.current.state as { runId: string }).runId).toBe("run-5");
  });

  it("hydrate with failed status sets failed state with error message", () => {
    const { result } = renderHook(() => useAppBuilderRun({ projectId: "proj-1" }));
    act(() => {
      result.current.hydrate("run-5", "failed", [], "oops");
    });
    expect(result.current.state.tag).toBe("failed");
    expect((result.current.state as { message: string }).message).toBe("oops");
  });
});
