// @vitest-environment jsdom

/**
 * `useBackfillSuggestion` must reach a terminal state.
 *
 * The regression these pin: `loading` was defined as "the latest run has
 * reported no resources yet", which never becomes false for a run that failed
 * *before* it planned any — admission refusal, a connector build error, a
 * malformed deployment row. The backfill modal then sat on "Reading the source
 * contracts…" for as long as it stayed open: a message that reads as *in
 * progress* while it actually means *never*.
 *
 * The bound is the run stream's own end rather than a timer, so both directions
 * are asserted here: it stays `true` while the stream is genuinely open (a
 * timer short enough to rescue a dead stream would fire here, mid-replay), and
 * it clears the moment the stream ends with nothing.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type React from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  listRuns: vi.fn(),
  streamEvents: vi.fn()
}));

vi.mock("@/services/api/airway", () => ({
  AirwayService: {
    listRuns: mocks.listRuns,
    streamEvents: mocks.streamEvents
  }
}));

vi.mock("@/hooks/useCurrentProjectBranch", () => ({
  default: () => ({ project: { id: "project-1" }, branchName: "main" })
}));

import { useBackfillSuggestion } from "./useAirway";

type StreamOptions = {
  onEvent: (event: unknown) => void;
  onClose?: () => void;
  onError?: (error: Error) => void;
};

/** A stream that opens and is never spoken to again — a live replay in flight. */
function openForever() {
  mocks.streamEvents.mockImplementation(() => new Promise<void>(() => {}));
}

/** A stream that opens and closes having reported nothing: the failed-early run. */
function closesEmpty() {
  mocks.streamEvents.mockImplementation(async (_p: string, _r: string, o: StreamOptions) => {
    o.onClose?.();
  });
}

function wrapper({ children }: { children: React.ReactNode }) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } }
  });
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.listRuns.mockResolvedValue([{ run_id: "run-1" }]);
  openForever();
});

describe("useBackfillSuggestion", () => {
  it("clears `loading` when the run's stream ends without reporting a resource", async () => {
    closesEmpty();
    const { result } = renderHook(() => useBackfillSuggestion("pipe", true), { wrapper });

    await waitFor(() => expect(result.current.loading).toBe(false));
    // …and having settled, it says so honestly rather than suggesting a window.
    expect(result.current.suggestion.window).toBeNull();
    expect(result.current.neverRan).toBe(false);
    expect(result.current.runsError).toBe(false);
  });

  it("keeps `loading` while the stream is open and has not reported yet", async () => {
    const { result } = renderHook(() => useBackfillSuggestion("pipe", true), { wrapper });

    await waitFor(() => expect(mocks.streamEvents).toHaveBeenCalled());
    // Never flips to a terminal message on the way to opening the stream: the
    // pre-subscribe render is `idle`, which is not settled.
    expect(result.current.loading).toBe(true);
  });

  it("clears `loading` when the stream fails, which reports no close of its own", async () => {
    mocks.streamEvents.mockImplementation(async (_p: string, _r: string, o: StreamOptions) => {
      // `fetchEventSource` treats a throw from `onerror` as fatal and does
      // not then call `onclose`, so a consumer that waits only on close
      // waits forever.
      o.onError?.(new Error("SSE connection failed with status: 502"));
    });
    const { result } = renderHook(() => useBackfillSuggestion("pipe", true), { wrapper });

    await waitFor(() => expect(result.current.loading).toBe(false));
  });

  it("reports a failed run-history read as an error, not as 'has not run yet'", async () => {
    mocks.listRuns.mockRejectedValue(new Error("boom"));
    const { result } = renderHook(() => useBackfillSuggestion("pipe", true), { wrapper });

    await waitFor(() => expect(result.current.runsError).toBe(true));
    expect(result.current.neverRan).toBe(false);
    expect(result.current.loading).toBe(false);
  });

  it("still reports a pipeline that has genuinely never run", async () => {
    mocks.listRuns.mockResolvedValue([]);
    const { result } = renderHook(() => useBackfillSuggestion("pipe", true), { wrapper });

    await waitFor(() => expect(result.current.neverRan).toBe(true));
    expect(result.current.runsError).toBe(false);
    expect(result.current.loading).toBe(false);
    expect(mocks.streamEvents).not.toHaveBeenCalled();
  });
});
