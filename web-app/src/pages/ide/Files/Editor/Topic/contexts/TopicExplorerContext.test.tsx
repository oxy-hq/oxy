// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { TopicExplorerProvider, useTopicExplorerContext } from "./TopicExplorerContext";

const useTopicDetails = vi.fn();
vi.mock("@/hooks/api/useSemanticQuery", () => ({
  useTopicDetails: (pathb64: string | undefined) => useTopicDetails(pathb64),
  useExecuteSemanticQuery: () => ({ mutate: vi.fn(), isPending: false }),
  useCompileSemanticQuery: () => ({ mutate: vi.fn(), isPending: false })
}));

function Probe() {
  useTopicExplorerContext();
  return <div data-testid='probe-ok' />;
}

describe("TopicExplorerProvider", () => {
  afterEach(() => useTopicDetails.mockReset());

  it("uses the explicit pathb64 prop without an editor context", () => {
    useTopicDetails.mockReturnValue({
      data: undefined,
      isLoading: false,
      error: undefined,
      refetch: vi.fn()
    });
    render(
      <TopicExplorerProvider pathb64='dG9waWM='>
        <Probe />
      </TopicExplorerProvider>
    );
    expect(screen.getByTestId("probe-ok")).toBeInTheDocument();
    expect(useTopicDetails).toHaveBeenCalledWith("dG9waWM=");
  });
});
