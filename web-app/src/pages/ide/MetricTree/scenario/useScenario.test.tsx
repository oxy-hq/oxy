// @vitest-environment jsdom

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MetricTree } from "@/types/metricTree";
import type { ScenarioState } from "./scenarioUrl";

const mockBaseline = vi.fn();
const mockPredictMutate = vi.fn();
const mockPredict = vi.fn();

vi.mock("@/hooks/api/useMetricTree", () => ({
  useBaseline: (...args: unknown[]) => mockBaseline(...args),
  usePredict: () => mockPredict()
}));

import { useScenario } from "./useScenario";

const tree = {
  nodes: [{ id: "a" }, { id: "b" }, { id: "far" }],
  edges: [{ from: "a", to: "b" }]
} as unknown as MetricTree;

const baseState = {
  levers: [{ nodeId: "a", raw: "11" }],
  periodDays: 90,
  timeDimension: "orders.order_date",
  instance: null
};

/**
 * Fires the pending debounced predict call and resolves it as the network
 * would: through the `onSuccess` the hook attaches to that specific `mutate`
 * call, not by mutating `usePredict()`'s return value directly. Requires
 * fake timers to already be active.
 */
function resolvePredict(data: { inputs: unknown[]; impacts: unknown[] }) {
  vi.advanceTimersByTime(300);
  const lastCall = mockPredictMutate.mock.calls.at(-1);
  act(() => {
    lastCall?.[1]?.onSuccess(data);
  });
}

// Scoped to the module, not one `describe`, so it resets between the two
// describe blocks below too — without it, a mutate call made real by fake
// timers in the last test of the first block was still sitting in
// `mockPredictMutate.mock.calls` when the second block's test asserted
// against it.
afterEach(cleanup);

beforeEach(() => {
  mockBaseline.mockReturnValue({ data: undefined, isPending: false, error: null });
  mockPredict.mockReturnValue({
    mutate: mockPredictMutate,
    reset: vi.fn(),
    data: undefined,
    isPending: false,
    error: null
  });
  mockPredictMutate.mockClear();
});

describe("useScenario", () => {
  it("issues no request when levers conflict", () => {
    const conflicting = {
      ...baseState,
      levers: [
        { nodeId: "a", raw: "11" },
        { nodeId: "b", raw: "5" }
      ]
    };
    const { result } = renderHook(() => useScenario({ tree, state: conflicting }));
    expect(result.current.conflicts).toEqual([{ upstream: "a", downstream: "b" }]);
    // The baseline hook is called with null — i.e. disabled — and predict never fires.
    expect(mockBaseline).toHaveBeenCalledWith(null);
    expect(mockPredictMutate).not.toHaveBeenCalled();
  });

  // The classic stale-response race: the lever moves twice before the first
  // predict call answers. Request A (the abandoned "20") must not clobber
  // request B (the current "30") just because A happens to settle later.
  it("keeps the current predict response when a slower, superseded one arrives late", () => {
    vi.useFakeTimers();
    try {
      mockBaseline.mockReturnValue({
        data: { values: { a: 14, b: 100 }, unvalued: [], resolved_period: ["x", "y"] },
        isPending: false,
        error: null
      });
      const stateAt20: ScenarioState = { ...baseState, levers: [{ nodeId: "a", raw: "20" }] };
      const stateAt30: ScenarioState = { ...baseState, levers: [{ nodeId: "a", raw: "30" }] };

      const { result, rerender } = renderHook(
        ({ state }: { state: ScenarioState }) => useScenario({ tree, state }),
        { initialProps: { state: stateAt20 } }
      );

      // Request A fires for the lever at 20.
      vi.advanceTimersByTime(300);
      expect(mockPredictMutate).toHaveBeenCalledTimes(1);
      const requestA = mockPredictMutate.mock.calls[0];

      // The user moves the lever again before A resolves — request B fires
      // for 30, the scenario now on screen.
      rerender({ state: stateAt30 });
      vi.advanceTimersByTime(300);
      expect(mockPredictMutate).toHaveBeenCalledTimes(2);
      const requestB = mockPredictMutate.mock.calls[1];

      // B is fast and resolves first, for the current scenario.
      act(() => {
        requestB[1].onSuccess({
          inputs: [],
          impacts: [
            { measure: "b", estimated_delta: 999, confidence: "high", path: [], form: "linear" }
          ]
        });
      });
      expect(result.current.nodeData.get("b")?.delta).toBe(999);

      // A is slow and finally resolves late, for a scenario the user has
      // already moved past. It must not overwrite B's result on screen.
      act(() => {
        requestA[1].onSuccess({
          inputs: [],
          impacts: [
            { measure: "b", estimated_delta: 111, confidence: "high", path: [], form: "linear" }
          ]
        });
      });
      expect(result.current.nodeData.get("b")?.delta).toBe(999);
    } finally {
      vi.useRealTimers();
    }
  });

  it("sends the baseline's fitted coefficients into predict", async () => {
    // Fitting is a warehouse query keyed on the lever set and window; predict
    // re-runs on every keystroke. If the fits did not ride along, each
    // keystroke would either re-measure them or silently lose them.
    const fitted = [{ from: "a", to: "b", coefficient: 5.78, lag: 7, t_stat: 36.5 }];
    mockBaseline.mockReturnValue({
      data: { values: { a: 14, b: 100 }, unvalued: [], resolved_period: ["x", "y"], fitted },
      isPending: false,
      error: null
    });
    vi.useFakeTimers();
    renderHook(() => useScenario({ tree, state: baseState }));
    await vi.advanceTimersByTimeAsync(400);
    vi.useRealTimers();

    expect(mockPredictMutate.mock.calls[0][0]).toEqual(
      expect.objectContaining({ coefficients: fitted })
    );
  });

  it("exposes the baseline's fits and refusals", () => {
    // A refusal is the only explanation for a branch of the canvas showing
    // nothing. Dropping it here would leave the UI silent about its silence.
    const fitted = [{ from: "a", to: "b", refusal: "no reliable relationship in this window" }];
    mockBaseline.mockReturnValue({
      data: { values: { a: 14 }, unvalued: [], resolved_period: ["x", "y"], fitted },
      isPending: false,
      error: null
    });
    const { result } = renderHook(() => useScenario({ tree, state: baseState }));
    expect(result.current.fitted).toEqual(fitted);
  });

  it("marks a reachable but unmoved node unchanged, not impacted", () => {
    // The baseline values everything forward-reachable from a lever, so a node
    // downstream of an edge that could not be sized is valued but carries no
    // impact. Filing it as "impacted" rendered a highlighted node with an empty
    // body and a blank row in the impact list.
    mockBaseline.mockReturnValue({
      data: { values: { a: 14, b: 100 }, unvalued: [], resolved_period: ["x", "y"] },
      isPending: false,
      error: null
    });
    mockPredict.mockReturnValue({
      mutate: mockPredictMutate,
      reset: vi.fn(),
      data: { impacts: [] },
      isPending: false,
      error: null
    });
    const { result } = renderHook(() => useScenario({ tree, state: baseState }));

    const b = result.current.nodeData.get("b");
    expect(b?.state).toBe("unchanged");
    expect(b?.baseline).toBe(100);
    // Nothing to render as a change, which is why it must not claim to be one.
    expect(b?.delta).toBeUndefined();
    expect(b?.simulated).toBeUndefined();
  });

  it("marks nodes outside the reachable set unreachable", () => {
    mockBaseline.mockReturnValue({
      data: { values: { a: 14, b: 100 }, unvalued: [], resolved_period: ["x", "y"] },
      isPending: false,
      error: null
    });
    const { result } = renderHook(() => useScenario({ tree, state: baseState }));
    expect(result.current.nodeData.get("far")?.state).toBe("unreachable");
    expect(result.current.unreachableCount).toBe(1);
  });

  it("marks the lever as a lever and carries what was typed", () => {
    mockBaseline.mockReturnValue({
      data: { values: { a: 14, b: 100 }, unvalued: [], resolved_period: ["x", "y"] },
      isPending: false,
      error: null
    });
    const { result } = renderHook(() => useScenario({ tree, state: baseState }));
    expect(result.current.nodeData.get("a")).toMatchObject({
      state: "lever",
      baseline: 14,
      leverRaw: "11"
    });
  });

  // The lever's own move is resolved client-side and never appears in
  // `predict`'s impacts — the engine reports what a change causes, not the
  // change itself. Leaving it off the node data is what left every lever
  // surface showing the baseline it had just been moved away from.
  it("carries the lever's own resolved change, not just its baseline", () => {
    mockBaseline.mockReturnValue({
      data: { values: { a: 14, b: 100 }, unvalued: [], resolved_period: ["x", "y"] },
      isPending: false,
      error: null
    });
    const { result } = renderHook(() => useScenario({ tree, state: baseState }));
    expect(result.current.nodeData.get("a")).toMatchObject({
      state: "lever",
      baseline: 14,
      simulated: 11,
      delta: -3
    });
  });

  it("carries a lever's delta with no baseline to add it to", () => {
    const deltaOnly = { ...baseState, timeDimension: null, levers: [{ nodeId: "a", raw: "-3" }] };
    const { result } = renderHook(() => useScenario({ tree, state: deltaOnly }));
    expect(result.current.nodeData.get("a")).toMatchObject({ state: "lever", delta: -3 });
    expect(result.current.nodeData.get("a")?.simulated).toBeUndefined();
  });

  // A lever seeded with its own baseline resolves to `no_change`, which is not
  // an error and not a movement. It must not arrive as `delta: 0`, or every
  // freshly pinned lever claims a 0.0% move it never made.
  it("carries no delta for a lever that resolves to no change", () => {
    mockBaseline.mockReturnValue({
      data: { values: { a: 14 }, unvalued: [], resolved_period: ["x", "y"] },
      isPending: false,
      error: null
    });
    const unmoved = { ...baseState, levers: [{ nodeId: "a", raw: "14" }] };
    const { result } = renderHook(() => useScenario({ tree, state: unmoved }));
    const a = result.current.nodeData.get("a");
    expect(a).toMatchObject({ state: "lever", baseline: 14 });
    expect(a?.delta).toBeUndefined();
    expect(a?.simulated).toBeUndefined();
  });

  it("maps an unquantifiable impact to the unquantifiable state with no simulated value", () => {
    vi.useFakeTimers();
    try {
      mockBaseline.mockReturnValue({
        data: { values: { a: 14, b: 100 }, unvalued: [], resolved_period: ["x", "y"] },
        isPending: false,
        error: null
      });
      const { result } = renderHook(() => useScenario({ tree, state: baseState }));
      resolvePredict({
        inputs: [],
        impacts: [
          {
            measure: "b",
            estimated_delta: 0,
            confidence: "unquantifiable",
            path: [],
            form: "linear"
          }
        ]
      });
      const b = result.current.nodeData.get("b");
      expect(b?.state).toBe("unquantifiable");
      expect(b?.simulated).toBeUndefined();
    } finally {
      vi.useRealTimers();
    }
  });

  it("surfaces a lever error instead of propagating it", () => {
    mockBaseline.mockReturnValue({
      data: { values: { a: 0 }, unvalued: [], resolved_period: ["x", "y"] },
      isPending: false,
      error: null
    });
    const percentOnZero = { ...baseState, levers: [{ nodeId: "a", raw: "+10%" }] };
    const { result } = renderHook(() => useScenario({ tree, state: percentOnZero }));
    expect(result.current.leverErrors.get("a")).toBe("zero_baseline");
    expect(mockPredictMutate).not.toHaveBeenCalled();
  });

  it("marks a reachable node the baseline could not value", () => {
    mockBaseline.mockReturnValue({
      data: {
        values: { a: 14 },
        unvalued: [{ node_id: "b", reason: "no_rows_in_window" }],
        resolved_period: ["x", "y"]
      },
      isPending: false,
      error: null
    });
    const { result } = renderHook(() => useScenario({ tree, state: baseState }));
    expect(result.current.nodeData.get("b")).toMatchObject({
      state: "unvalued",
      unvaluedReason: "no_rows_in_window"
    });
  });

  // A percentage lever against a refused baseline resolves to `no_baseline`,
  // which drops it from `changes`, which means `predict` is never called. The
  // impact list then has an empty result set that means "we never asked" —
  // and no way to know that without this flag.
  it("reports that no lever resolved when a % lever has no baseline to scale", () => {
    vi.useFakeTimers();
    const percentLever: ScenarioState = {
      ...baseState,
      levers: [{ nodeId: "a", raw: "+34%" }]
    };
    mockBaseline.mockReturnValue({
      data: {
        values: {},
        unvalued: [{ node_id: "a", reason: "query_failed" }],
        resolved_period: ["x", "y"],
        baseline_note: "the warehouse rejected the query: boom"
      },
      isPending: false,
      error: null
    });
    try {
      const { result } = renderHook(() => useScenario({ tree, state: percentLever }));
      vi.advanceTimersByTime(300);

      expect(result.current.leverErrors.get("a")).toBe("no_baseline");
      expect(mockPredictMutate).not.toHaveBeenCalled();
      expect(result.current.runState).toBe("unresolved");
    } finally {
      vi.useRealTimers();
    }
  });

  it("propagates a signed delta with no time dimension and no baseline (delta-only mode)", () => {
    vi.useFakeTimers();
    const noTimeDim: ScenarioState = {
      ...baseState,
      timeDimension: null,
      levers: [{ nodeId: "a", raw: "+3" }]
    };
    try {
      const { result } = renderHook(() => useScenario({ tree, state: noTimeDim }));

      // No time dimension means no baseline request is issued at all.
      expect(mockBaseline).toHaveBeenCalledWith(null);

      vi.advanceTimersByTime(300);

      expect(mockPredictMutate).toHaveBeenCalledTimes(1);
      const call = mockPredictMutate.mock.calls[0][0];
      expect(call).toEqual({ changes: [{ measure: "a", delta: 3 }] });
      expect(call).not.toHaveProperty("values");

      // The lever still folds correctly with no baseline value available.
      expect(result.current.nodeData.get("a")).toMatchObject({
        state: "lever",
        baseline: undefined,
        leverRaw: "+3"
      });
      // A signed delta is exactly the form that survives a missing baseline,
      // so this scenario DID run — the impact list must not disclaim it.
      expect(result.current.runState).toBe("ran");
    } finally {
      vi.useRealTimers();
    }
  });

  // A wrongly-picked time dimension made the baseline query fail, which marked
  // every reachable node `unvalued`. Because `unvalued` outranked `impacted`,
  // the genuinely-moved measures vanished from the impact list and the panel
  // claimed the lever moved nothing.
  it("keeps an impacted measure impacted when the baseline query failed", () => {
    vi.useFakeTimers();
    try {
      mockBaseline.mockReturnValue({
        data: {
          values: {},
          unvalued: [
            { node_id: "a", reason: "query_failed" },
            { node_id: "b", reason: "query_failed" }
          ],
          resolved_period: ["x", "y"],
          baseline_note: "the warehouse rejected the query: boom"
        },
        isPending: false,
        error: null
      });
      // A signed delta is the one lever form that still resolves with no
      // baseline to read — an absolute target on this failed baseline would
      // resolve to `no_baseline` and predict would never be called at all.
      const signedLever = { ...baseState, levers: [{ nodeId: "a", raw: "+3" }] };
      const { result } = renderHook(() => useScenario({ tree, state: signedLever }));
      resolvePredict({
        inputs: [],
        impacts: [
          { measure: "b", estimated_delta: 7, confidence: "exact", path: [], form: "linear" }
        ]
      });

      expect(result.current.nodeData.get("b")?.state).toBe("impacted");
      expect(result.current.nodeData.get("b")?.delta).toBe(7);
      // Surfaced verbatim from the server: the client cannot tell an executor
      // error from an empty window, and guessing produced the wrong advice.
      expect(result.current.baselineNote).toContain("warehouse rejected");
    } finally {
      vi.useRealTimers();
    }
  });

  it("folds a delta-only downstream impact to impacted rather than unreachable", () => {
    vi.useFakeTimers();
    try {
      const noTimeDim: ScenarioState = {
        ...baseState,
        timeDimension: null,
        levers: [{ nodeId: "a", raw: "+3" }]
      };
      // No baseline in delta-only mode (default mockBaseline stays `data: undefined`).
      const { result } = renderHook(() => useScenario({ tree, state: noTimeDim }));
      resolvePredict({
        inputs: [],
        impacts: [
          { measure: "b", estimated_delta: 5, confidence: "high", path: [], form: "linear" }
        ]
      });

      expect(result.current.nodeData.get("b")?.state).toBe("impacted");
      // The delta must survive the fold: with no baseline it is the only number
      // the node can render, and dropping it made a pinned lever look inert.
      expect(result.current.nodeData.get("b")?.delta).toBe(5);
      expect(result.current.nodeData.get("b")?.simulated).toBeUndefined();
      // "far" has no lever, no baseline, and no impact — genuinely untouched.
      expect(result.current.nodeData.get("far")?.state).toBe("unreachable");
      expect(result.current.unreachableCount).toBe(1);
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("useScenario — why the impact list is empty", () => {
  // The three cases need three different sentences, and the two-way flag they
  // replaced conflated the middle one with "propagation ran and found
  // nothing". A freshly pinned lever prefills from its baseline, so it lands
  // in `unmoved` — and used to be told "this lever moves no other modelled
  // measure", a modelling claim about a run that never happened.
  it("calls a lever sitting at its current value unmoved, not resolved", () => {
    vi.useFakeTimers();
    const atBaseline: ScenarioState = { ...baseState, levers: [{ nodeId: "a", raw: "14" }] };
    mockBaseline.mockReturnValue({
      data: { values: { a: 14, b: 100 }, unvalued: [], resolved_period: ["x", "y"] },
      isPending: false,
      error: null
    });
    try {
      const { result } = renderHook(() => useScenario({ tree, state: atBaseline }));
      vi.advanceTimersByTime(300);

      expect(result.current.leverErrors.size).toBe(0);
      expect(mockPredictMutate).not.toHaveBeenCalled();
      expect(result.current.runState).toBe("unmoved");
    } finally {
      vi.useRealTimers();
    }
  });
});
