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
    layoutWithElk: vi.fn(async (nodes) => ({ nodes, waypointMap: new Map() }))
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
  Controls: () => null,
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
      is_composite: false,
      drillable: false
    },
    {
      id: "orders.profit",
      view: "orders",
      measure: "profit",
      label: "Profit",
      measure_type: "number",
      is_composite: true,
      drillable: false
    }
  ],
  edges: [
    { from: "orders.revenue", to: "orders.profit", kind: "component" } as MetricTree["edges"][0]
  ]
};

/** `tree` plus a measure no edge touches. The base fixture is fully connected,
 *  so the orphan filter is a no-op on it and every assertion there passes under
 *  either default — which is what let the default go untested. */
const treeWithOrphan: MetricTree = {
  ...tree,
  nodes: [
    ...tree.nodes,
    {
      id: "orders.headcount",
      view: "orders",
      measure: "headcount",
      label: "Headcount",
      measure_type: "sum",
      is_composite: false,
      drillable: false
    }
  ]
};

describe("MetricTreeGraph", () => {
  afterEach(cleanup);

  it("renders a node per measure", async () => {
    render(<MetricTreeGraph tree={tree} selectedId={null} onSelect={vi.fn()} />);
    expect(await screen.findByText("Revenue")).toBeInTheDocument();
    expect(screen.getByText("Profit")).toBeInTheDocument();
  });

  // The default is load-bearing beyond layout: it decides how many nodes an
  // agentic browser flow has to click through to find one with drivers, which
  // `_budgets.yml` cites as the reason `metric-tree-scenario`'s cost ceiling can
  // be tight. On the demo layer this is the difference between 2 candidates and
  // 53. Flipping `useState(true)` to `false` was a green diff before this test.
  it("hides unconnected measures by default", async () => {
    render(<MetricTreeGraph tree={treeWithOrphan} selectedId={null} onSelect={vi.fn()} />);
    expect(await screen.findByText("Revenue")).toBeInTheDocument();
    expect(screen.getByText("Profit")).toBeInTheDocument();
    expect(screen.queryByText("Headcount")).not.toBeInTheDocument();
  });

  it("shows unconnected measures once the toggle is switched off", async () => {
    render(<MetricTreeGraph tree={treeWithOrphan} selectedId={null} onSelect={vi.fn()} />);
    await screen.findByText("Revenue");
    fireEvent.click(screen.getByLabelText(/Hide unconnected/i));
    expect(await screen.findByText("Headcount")).toBeInTheDocument();
  });

  // The filter's third state, and the one a fresh semantic layer actually shows:
  // every measure is an orphan, so hiding them leaves nothing to draw. Without
  // this the graph would render an empty canvas with no explanation.
  it("explains an empty canvas when every measure is unconnected", () => {
    render(
      <MetricTreeGraph
        tree={{ ...treeWithOrphan, edges: [] }}
        selectedId={null}
        onSelect={vi.fn()}
      />
    );
    expect(screen.getByText("All measures are unconnected.")).toBeInTheDocument();
    expect(screen.queryByText("Revenue")).not.toBeInTheDocument();
    // The half that makes the state actionable. This is the only branch whose
    // copy tells the user to use a control the same branch renders, so a
    // refactor dropping `{orphanToggle}` as boilerplate would leave "toggle X"
    // with no X — and without these two, nothing would fail.
    expect(screen.getByText(/Toggle "Hide unconnected" off/)).toBeInTheDocument();
    expect(screen.getByLabelText(/Hide unconnected/i)).toBeInTheDocument();
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
