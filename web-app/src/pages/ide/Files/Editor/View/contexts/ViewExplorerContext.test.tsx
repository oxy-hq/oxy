// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useViewExplorerContext, ViewExplorerProvider } from "./ViewExplorerContext";

const useViewDetails = vi.fn();
vi.mock("@/hooks/api/useSemanticQuery", () => ({
  useViewDetails: (pathb64: string | undefined) => useViewDetails(pathb64),
  useExecuteSemanticQuery: () => ({ mutate: vi.fn(), isPending: false }),
  useCompileSemanticQuery: () => ({ mutate: vi.fn(), isPending: false })
}));

function Probe() {
  useViewExplorerContext();
  return <div data-testid='probe-ok' />;
}

describe("ViewExplorerProvider", () => {
  afterEach(() => useViewDetails.mockReset());

  it("uses the explicit pathb64 prop without an editor context", () => {
    useViewDetails.mockReturnValue({
      data: undefined,
      isLoading: false,
      error: undefined,
      refetch: vi.fn()
    });
    render(
      <ViewExplorerProvider pathb64='dmlldw=='>
        <Probe />
      </ViewExplorerProvider>
    );
    expect(screen.getByTestId("probe-ok")).toBeInTheDocument();
    expect(useViewDetails).toHaveBeenCalledWith("dmlldw==");
  });
});
