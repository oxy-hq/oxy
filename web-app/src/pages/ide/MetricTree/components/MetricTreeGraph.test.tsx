// @vitest-environment jsdom

import { cleanup, findByTestId, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { MetricNode, MetricTree } from "@/types/metricTree";
import { MetricTreeGraph } from "./MetricTreeGraph";

// ELK runs async; the component holds back rendering until layout completes.
// Mock it to a pass-through so the test assertions see the rendered nodes
// immediately after the next microtask.
vi.mock("../graphLayout", async () => {
  const actual = await vi.importActual<typeof import("../graphLayout")>("../graphLayout");
  return {
    ...actual,
    layoutWithElk: vi.fn(async (nodes) => nodes)
  };
});

// ReactFlow needs browser APIs jsdom lacks — mock it to a flat node list.
vi.mock("@xyflow/react", () => ({
  ReactFlow: ({
    nodes,
    edges,
    onNodeClick
  }: {
    nodes: Array<{ id: string; data: { node: MetricNode } }>;
    edges: Array<{ id: string; className?: string }>;
    onNodeClick: (event: React.MouseEvent, node: { id: string }) => void;
  }) => (
    <div data-testid='react-flow'>
      {nodes.map((n) => (
        <button
          type='button'
          key={n.id}
          data-testid={`rf-node-${n.id}`}
          onClick={(e) => onNodeClick(e, n)}
        >
          {n.data.node.label}
        </button>
      ))}
      {edges.map((e) => (
        <div key={e.id} data-testid={`rf-edge-${e.id}`} className={e.className} />
      ))}
    </div>
  ),
  Background: () => null,
  BackgroundVariant: { Dots: "dots" },
  Handle: () => null,
  Position: { Top: "top", Bottom: "bottom" }
}));

const tree: MetricTree = {
  nodes: [
    {
      id: "orders.revenue",
      view: "orders",
      measure: "revenue",
      label: "Revenue",
      measure_type: "sum",
      is_composite: false
    },
    {
      id: "orders.profit",
      view: "orders",
      measure: "profit",
      label: "Profit",
      measure_type: "number",
      is_composite: true
    }
  ],
  edges: [
    { from: "orders.revenue", to: "orders.profit", kind: "component" } as MetricTree["edges"][0]
  ]
};

describe("MetricTreeGraph", () => {
  afterEach(cleanup);

  it("renders a node per measure", async () => {
    render(<MetricTreeGraph tree={tree} selectedId={null} onSelect={vi.fn()} />);
    expect(await screen.findByText("Revenue")).toBeInTheDocument();
    expect(screen.getByText("Profit")).toBeInTheDocument();
  });

  it("calls onSelect with the node id when a node is clicked", async () => {
    const onSelect = vi.fn();
    const { container } = render(
      <MetricTreeGraph tree={tree} selectedId={null} onSelect={onSelect} />
    );
    fireEvent.click(await findByTestId(container, "rf-node-orders.profit"));
    expect(onSelect).toHaveBeenCalledWith("orders.profit");
  });

  it("renders an empty state when the tree has no measures", () => {
    render(
      <MetricTreeGraph tree={{ nodes: [], edges: [] }} selectedId={null} onSelect={vi.fn()} />
    );
    expect(screen.getByText(/No measures found/i)).toBeInTheDocument();
  });
});
