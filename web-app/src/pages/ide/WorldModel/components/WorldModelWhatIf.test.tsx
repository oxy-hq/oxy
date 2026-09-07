// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SensitivityDriver } from "@/types/metricTree";
import { WorldModelWhatIf } from "./WorldModelWhatIf";

const mockPredictMutate = vi.fn();
const mockPredict = vi.fn();

vi.mock("@/hooks/api/useMetricTree", () => ({
  usePredict: () => mockPredict()
}));

const drivers: SensitivityDriver[] = [
  {
    measure: "orders.aov",
    path: ["orders.aov", "orders.revenue"],
    edge_kind: "component",
    effective_coefficient: 1,
    direction: "positive",
    strength: "strong"
  }
];

afterEach(cleanup);

beforeEach(() => {
  mockPredict.mockReturnValue({
    mutate: mockPredictMutate,
    reset: vi.fn(),
    data: undefined,
    isPending: false,
    error: null
  });
  mockPredictMutate.mockClear();
});

function renderWhatIf() {
  return render(<WorldModelWhatIf drivers={drivers} target='orders.revenue' />);
}

describe("WorldModelWhatIf", () => {
  // The classic stale-response race: the delta changes twice before the first
  // predict call answers. Request A (the abandoned delta) must not clobber
  // request B (the current delta) just because A happens to settle later.
  it("keeps the current predict response when a slower, superseded one arrives late", () => {
    vi.useFakeTimers();
    try {
      renderWhatIf();
      const input = screen.getByTestId("wm-whatif-delta");

      // Request A fires for delta = 2.
      fireEvent.change(input, { target: { value: "2" } });
      vi.advanceTimersByTime(300);
      expect(mockPredictMutate).toHaveBeenCalledTimes(1);
      const requestA = mockPredictMutate.mock.calls[0];

      // The user changes the delta again before A resolves — request B fires
      // for 3, the value now on screen.
      fireEvent.change(input, { target: { value: "3" } });
      vi.advanceTimersByTime(300);
      expect(mockPredictMutate).toHaveBeenCalledTimes(2);
      const requestB = mockPredictMutate.mock.calls[1];

      // B is fast and resolves first, for the current input.
      act(() => {
        requestB[1].onSuccess({
          inputs: [],
          impacts: [
            {
              measure: "orders.revenue",
              estimated_delta: 999,
              confidence: "exact",
              path: [],
              form: "linear"
            }
          ]
        });
      });
      expect(screen.getByText("+999.0")).toBeInTheDocument();

      // A is slow and finally resolves late, for a delta the user has already
      // moved past. It must not overwrite B's result on screen.
      act(() => {
        requestA[1].onSuccess({
          inputs: [],
          impacts: [
            {
              measure: "orders.revenue",
              estimated_delta: 111,
              confidence: "exact",
              path: [],
              form: "linear"
            }
          ]
        });
      });
      expect(screen.getByText("+999.0")).toBeInTheDocument();
      expect(screen.queryByText("+111.0")).not.toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("does not let a late stale response revive a pending spinner after the input is cleared", () => {
    vi.useFakeTimers();
    try {
      renderWhatIf();
      const input = screen.getByTestId("wm-whatif-delta");

      fireEvent.change(input, { target: { value: "2" } });
      vi.advanceTimersByTime(300);
      expect(mockPredictMutate).toHaveBeenCalledTimes(1);
      const requestA = mockPredictMutate.mock.calls[0];

      // The user clears the delta — the pending request is disowned, and the
      // UI must show no result and no error for it going forward.
      fireEvent.change(input, { target: { value: "0" } });

      // The now-abandoned request resolves late.
      act(() => {
        requestA[1].onSuccess({
          inputs: [],
          impacts: [
            {
              measure: "orders.revenue",
              estimated_delta: 111,
              confidence: "exact",
              path: [],
              form: "linear"
            }
          ]
        });
      });

      expect(screen.queryByText("+111.0")).not.toBeInTheDocument();
      expect(screen.queryByText(/does not propagate/)).not.toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });
});
