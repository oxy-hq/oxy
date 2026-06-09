// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import SemanticObjectsList from "./SemanticObjectsList";

vi.mock("@/hooks/api/useTopicFiles", () => ({
  default: () => ({
    topicFiles: [
      {
        value: "sales",
        label: "sales",
        path: "semantics/topics/sales.topic.yml",
        searchText: "sales"
      }
    ],
    isLoading: false,
    error: null
  })
}));
vi.mock("@/hooks/api/useViewFiles", () => ({
  default: () => ({
    viewFiles: [
      {
        value: "orders",
        label: "orders",
        path: "semantics/views/orders.view.yml",
        searchText: "orders"
      }
    ],
    isLoading: false,
    error: null
  })
}));

describe("SemanticObjectsList", () => {
  afterEach(cleanup);

  it("lists topics and views together in a flat list", () => {
    render(<SemanticObjectsList selectedPath={null} onSelect={vi.fn()} />);
    expect(screen.getByText("sales")).toBeInTheDocument();
    expect(screen.getByText("orders")).toBeInTheDocument();
  });

  it("calls onSelect with the clicked item", () => {
    const onSelect = vi.fn();
    render(<SemanticObjectsList selectedPath={null} onSelect={onSelect} />);
    fireEvent.click(screen.getByTestId("semantic-objects-item-view-orders"));
    expect(onSelect).toHaveBeenCalledWith({
      kind: "view",
      label: "orders",
      path: "semantics/views/orders.view.yml"
    });
  });
});
