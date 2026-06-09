// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import SemanticLayerPage from "./index";

vi.mock("./SemanticExplorerTab", () => ({
  default: () => <div data-testid='explorer-tab' />
}));
vi.mock("../MetricTree", () => ({
  default: () => <div data-testid='metric-tree-tab' />
}));

describe("SemanticLayerPage", () => {
  afterEach(cleanup);

  it("renders both tab triggers, Explorer active by default", () => {
    render(
      <MemoryRouter initialEntries={["/ide/semantic"]}>
        <SemanticLayerPage />
      </MemoryRouter>
    );
    expect(screen.getByRole("tab", { name: "Explorer" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Metric Tree" })).toBeInTheDocument();
    expect(screen.getByTestId("explorer-tab")).toBeInTheDocument();
  });

  it("selects the Metric Tree tab from ?view=metric-tree", () => {
    render(
      <MemoryRouter initialEntries={["/ide/semantic?view=metric-tree"]}>
        <SemanticLayerPage />
      </MemoryRouter>
    );
    expect(screen.getByTestId("metric-tree-tab")).toBeInTheDocument();
  });
});
