// @vitest-environment jsdom

import { renderHook, act, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useAnalyticsRun } from "./useAnalyticsRun";

// ── Module mocks ──────────────────────────────────────────────────────────────

let capturedOptions: {
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

vi.mock("@/services/api/analytics", () => ({
  AnalyticsService: {
    eventsUrl: (_projectId: string, runId: string) => `/api/projects/proj-1/runs/${runId}/events`,
    createRun: vi.fn().mockResolvedValue({ run_id: "run-1" }),
    cancelRun: vi.fn().mockResolvedValue(undefined),
    submitAnswer: vi.fn().mockResolvedValue(undefined)
  }
}));

// ── Tests ─────────────────────────────────────────────────────────────────────

beforeEach(() => {
  capturedOptions = {};
  localStorage.setItem("auth_token", "test-token");
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("useAnalyticsRun — onerror with intentional abort", () => {
  it("does not log to console when onerror fires after signal was aborted", async () => {
    const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    const { result } = renderHook(() => useAnalyticsRun({ projectId: "proj-1" }));

    act(() => {
      result.current.reconnect("run-1");
    });

    expect(result.current.state.tag).toBe("running");
    expect(capturedOptions.signal).toBeDefined();
    expect(capturedOptions.signal!.aborted).toBe(false);

    // Abort intentionally (simulates thread switch or stop)
    act(() => {
      result.current.reset();
    });

    expect(capturedOptions.signal!.aborted).toBe(true);
    expect(result.current.state.tag).toBe("idle");

    // Simulate fetchEventSource calling onerror after the abort
    const abortErr = new DOMException("The user aborted a request.", "AbortError");
    act(() => {
      try {
        capturedOptions.onerror?.(abortErr);
      } catch {
        // onerror re-throws — expected
      }
    });

    // State must remain idle, not transition to "failed"
    expect(result.current.state.tag).toBe("idle");
    // No error should be logged for an intentional abort
    expect(consoleSpy).not.toHaveBeenCalled();

    consoleSpy.mockRestore();
  });

  it("does log and sets failed state when onerror fires without an intentional abort", async () => {
    const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    const { result } = renderHook(() => useAnalyticsRun({ projectId: "proj-1" }));

    act(() => {
      result.current.reconnect("run-1");
    });

    expect(result.current.state.tag).toBe("running");

    // Simulate a real network error (signal NOT aborted)
    const networkErr = new Error("Network failure");
    act(() => {
      try {
        capturedOptions.onerror?.(networkErr);
      } catch {
        // onerror re-throws — expected
      }
    });

    expect(result.current.state.tag).toBe("failed");
    expect(consoleSpy).toHaveBeenCalledWith("Analytics SSE error:", networkErr);

    consoleSpy.mockRestore();
  });
});

describe("useAnalyticsRun — SSE cleanup on unmount", () => {
  it("aborts the SSE connection when the hook unmounts", () => {
    const { result, unmount } = renderHook(() => useAnalyticsRun({ projectId: "proj-1" }));

    act(() => {
      result.current.reconnect("run-1");
    });

    expect(capturedOptions.signal).toBeDefined();
    expect(capturedOptions.signal!.aborted).toBe(false);

    unmount();

    expect(capturedOptions.signal!.aborted).toBe(true);
  });
});
