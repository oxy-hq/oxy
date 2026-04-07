// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { ContextGraphEdge, ContextGraphNode } from "@/types/contextGraph";
import { GraphStatsPanel } from "./GraphStatsPanel";

afterEach(() => cleanup());

const makeNodes = (count: number, type: ContextGraphNode["type"] = "agent"): ContextGraphNode[] =>
  Array.from({ length: count }, (_, i) => ({
    id: `${type}-${i}`,
    type,
    label: `${type} ${i}`,
    data: { name: `${type}${i}` }
  }));

const makeEdges = (count: number): ContextGraphEdge[] =>
  Array.from({ length: count }, (_, i) => ({
    id: `e${i}`,
    source: `src-${i}`,
    target: `tgt-${i}`
  }));

describe("GraphStatsPanel", () => {
  it("shows correct total node count", () => {
    render(<GraphStatsPanel nodes={makeNodes(5)} edges={makeEdges(3)} typeCounts={{ agent: 5 }} />);
    expect(screen.getByTestId("context-graph-total-nodes")).toHaveTextContent("5");
  });

  it("shows correct total edge count", () => {
    render(<GraphStatsPanel nodes={makeNodes(2)} edges={makeEdges(7)} typeCounts={{ agent: 2 }} />);
    expect(screen.getByTestId("context-graph-total-edges")).toHaveTextContent("7");
  });

  it("renders per-type count rows", () => {
    render(
      <GraphStatsPanel
        nodes={[...makeNodes(3, "agent"), ...makeNodes(2, "table")]}
        edges={[]}
        typeCounts={{ agent: 3, table: 2 }}
      />
    );
    expect(screen.getByText("Agents:")).toBeInTheDocument();
    expect(screen.getByText("3")).toBeInTheDocument();
    expect(screen.getByText("Tables:")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
  });

  it("renders 'Context Graph Overview' heading", () => {
    render(<GraphStatsPanel nodes={[]} edges={[]} typeCounts={{}} />);
    expect(screen.getByText("Context Graph Overview")).toBeInTheDocument();
  });
});
