// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@xyflow/react", () => ({
  Panel: ({ children, className }: { children: React.ReactNode; className: string }) => (
    <div data-testid='rf-panel' className={className}>
      {children}
    </div>
  )
}));

const { GraphControlPanel } = await import("./index");

afterEach(() => cleanup());

const defaultProps = {
  nodes: [{ id: "n1", type: "agent" as const, label: "Agent", data: { name: "a" } }],
  edges: [{ id: "e1", source: "n1", target: "n2" }],
  typeCounts: { agent: 1 },
  focusType: "auto" as const,
  onFocusTypeChange: vi.fn(),
  expandAll: false,
  onExpandAllChange: vi.fn(),
  focusedNodeId: null as string | null,
  onReset: vi.fn()
};

describe("GraphControlPanel", () => {
  it("renders stats section", () => {
    render(<GraphControlPanel {...defaultProps} />);
    expect(screen.getByText("Context Graph Overview")).toBeInTheDocument();
    expect(screen.getByTestId("context-graph-total-nodes")).toHaveTextContent("1");
  });

  it("renders filter section", () => {
    render(<GraphControlPanel {...defaultProps} />);
    expect(screen.getByText("Focus View")).toBeInTheDocument();
    expect(screen.getByTestId("context-graph-filter-type")).toBeInTheDocument();
  });

  it("renders inside an RFPanel", () => {
    render(<GraphControlPanel {...defaultProps} />);
    expect(screen.getByTestId("rf-panel")).toBeInTheDocument();
  });
});
