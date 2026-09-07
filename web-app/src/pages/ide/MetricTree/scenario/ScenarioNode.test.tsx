// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { ReactFlowProvider } from "@xyflow/react";
import { afterEach, describe, expect, it } from "vitest";
import type { MetricNode } from "@/types/metricTree";
import type { ScenarioNodeData } from "./nodeValue";
import { ScenarioNode } from "./ScenarioNode";

const node = { id: "orders.revenue", label: "Revenue", measure: "revenue" } as MetricNode;

function renderNode(data: ScenarioNodeData) {
  return render(
    <ReactFlowProvider>
      {/* React Flow's `NodeProps` requires the full node record; the component
          reads only `data`, so the rest is filled in rather than asserted. */}
      <ScenarioNode
        {...({
          id: data.node.id,
          data: data as unknown as Record<string, unknown>,
          type: "scenario-measure",
          selected: false,
          dragging: false,
          draggable: false,
          selectable: false,
          deletable: false,
          zIndex: 0,
          isConnectable: false,
          positionAbsoluteX: 0,
          positionAbsoluteY: 0
        } as React.ComponentProps<typeof ScenarioNode>)}
      />
    </ReactFlowProvider>
  );
}

describe("ScenarioNode", () => {
  afterEach(cleanup);

  it("shows baseline → simulated for an exact impact", () => {
    renderNode({
      node,
      state: "impacted",
      baseline: 2_100_000,
      simulated: 2_410_000,
      confidence: "exact"
    });
    expect(screen.getByTestId("scenario-node-orders.revenue")).toHaveTextContent("2.1M");
    expect(screen.getByTestId("scenario-node-orders.revenue")).toHaveTextContent("2.41M");
    expect(screen.getByTestId("scenario-node-orders.revenue")).toHaveTextContent("+14.8%");
  });

  it("never renders 0 for an unquantifiable impact", () => {
    renderNode({
      node,
      state: "unquantifiable",
      baseline: 2_100_000,
      confidence: "unquantifiable"
    });
    const rendered = screen.getByTestId("scenario-node-orders.revenue");
    expect(rendered).toHaveTextContent("?");
    expect(rendered).toHaveTextContent(/can't size/i);
    expect(rendered).not.toHaveTextContent(/(^|[^.\d])0([^.\d]|$)/);
    expect(rendered).not.toHaveTextContent("+0.0%");
  });

  it("greys an unvalued node and says why", () => {
    renderNode({ node, state: "unvalued", unvaluedReason: "no_rows_in_window" });
    expect(screen.getByTestId("scenario-node-orders.revenue")).toHaveTextContent(/no rows/i);
  });

  // The lever is the one node whose new value the analyst chose, so it is the
  // one node where showing only the baseline is indefensible: the panel then
  // reports the number they typed over as if nothing had been typed.
  it("shows a lever moving from its baseline to its typed value", () => {
    renderNode({ node, state: "lever", baseline: 14, simulated: 11, delta: -3, leverRaw: "11" });
    const rendered = screen.getByTestId("scenario-node-orders.revenue");
    expect(rendered).toHaveTextContent("LEVER");
    expect(rendered).toHaveTextContent("14");
    expect(rendered).toHaveTextContent("11");
    expect(rendered).toHaveTextContent("-21.4%");
  });

  it("shows a lever's bare delta when there is no baseline to anchor it", () => {
    renderNode({ node, state: "lever", delta: -3, leverRaw: "-3" });
    const rendered = screen.getByTestId("scenario-node-orders.revenue");
    expect(rendered).toHaveTextContent("LEVER");
    expect(rendered).toHaveTextContent("Δ");
    expect(rendered).toHaveTextContent("-3");
  });

  // A freshly pinned lever is seeded with its own baseline, so it resolves to
  // no change at all. Rendering "→ 14 +0.0%" there would dress up a non-event.
  it("shows a lever's baseline alone until it actually moves", () => {
    renderNode({ node, state: "lever", baseline: 14, leverRaw: "14" });
    const rendered = screen.getByTestId("scenario-node-orders.revenue");
    expect(rendered).toHaveTextContent("14");
    expect(rendered).not.toHaveTextContent("%");
    expect(rendered).not.toHaveTextContent("→");
  });

  // Which numbers are arithmetic and which are forecasts is the difference
  // between "profit falls 50k" and "profit probably falls about 50k". A surface
  // that shows both in the same type is overstating half of them.
  it("marks an exactly-computed impact apart from an estimated one", () => {
    renderNode({
      node,
      state: "impacted",
      baseline: 100,
      simulated: 150,
      delta: 50,
      confidence: "exact"
    });
    expect(screen.getByTestId("scenario-confidence-exact")).toBeInTheDocument();
    expect(screen.queryByTestId("scenario-confidence-estimated")).not.toBeInTheDocument();
  });

  it("marks an estimated impact as estimated", () => {
    renderNode({
      node,
      state: "impacted",
      baseline: 100,
      simulated: 150,
      delta: 50,
      confidence: "estimated"
    });
    expect(screen.getByTestId("scenario-confidence-estimated")).toBeInTheDocument();
    expect(screen.queryByTestId("scenario-confidence-exact")).not.toBeInTheDocument();
  });

  it("marks confidence on a delta-only impact too", () => {
    renderNode({ node, state: "impacted", delta: 50, confidence: "estimated" });
    expect(screen.getByTestId("scenario-confidence-estimated")).toBeInTheDocument();
  });

  it("renders an unreachable node with no numbers at all", () => {
    renderNode({ node, state: "unreachable" });
    const rendered = screen.getByTestId("scenario-node-orders.revenue");
    // The raw measure name, not `label` — airlayer fills that with the
    // description, which is a sentence on any documented measure.
    expect(rendered).toHaveTextContent("revenue");
    expect(rendered).not.toHaveTextContent(/\d/);
  });

  // Delta-only mode: with no time dimension there is no baseline, so the
  // propagated `estimated_delta` is the ONLY number available. Rendering
  // nothing here is what made a pinned lever look inert.
  it("renders the bare delta when there is no baseline to anchor it", () => {
    renderNode({ node, state: "impacted", delta: -3, confidence: "exact" });
    const rendered = screen.getByTestId("scenario-node-orders.revenue");
    expect(rendered).toHaveTextContent("-3");
    expect(rendered).toHaveTextContent("Δ");
  });

  it("marks a positive bare delta with a sign", () => {
    renderNode({ node, state: "impacted", delta: 310_000, confidence: "estimated" });
    expect(screen.getByTestId("scenario-node-orders.revenue")).toHaveTextContent("+310K");
  });

  // A delta of exactly 0 is a real, known "no movement" — distinct from the
  // unquantifiable case, which must keep rendering "?" rather than any number.
  // Regression: trailing-zero stripping used to run on integers too, so
  // 310K rendered as "31K" — a silent 10x understatement.
  it("does not truncate a trailing zero out of a compacted number", () => {
    renderNode({ node, state: "impacted", baseline: 310_000, simulated: 620_000 });
    const rendered = screen.getByTestId("scenario-node-orders.revenue");
    expect(rendered).toHaveTextContent("310K");
    expect(rendered).toHaveTextContent("620K");
    expect(rendered).not.toHaveTextContent(/(^|[^\d])31K/);
  });

  it("still refuses to size an unquantifiable impact even with no baseline", () => {
    renderNode({ node, state: "unquantifiable", confidence: "unquantifiable" });
    const rendered = screen.getByTestId("scenario-node-orders.revenue");
    expect(rendered).toHaveTextContent("?");
    expect(rendered).not.toHaveTextContent("Δ");
  });
});
