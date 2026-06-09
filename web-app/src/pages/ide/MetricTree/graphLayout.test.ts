import { describe, expect, it } from "vitest";
import type { MetricTree } from "@/types/metricTree";
import { metricTreeToFlow } from "./graphLayout";

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
    },
    {
      id: "marketing.ad_spend",
      view: "marketing",
      measure: "ad_spend",
      label: "Ad Spend",
      measure_type: "sum",
      is_composite: false
    }
  ],
  edges: [
    { from: "orders.revenue", to: "orders.profit", kind: "component" } as MetricTree["edges"][0],
    {
      from: "marketing.ad_spend",
      to: "orders.revenue",
      kind: "driver"
    } as MetricTree["edges"][0]
  ]
};

describe("metricTreeToFlow", () => {
  it("maps every node and edge", () => {
    const { nodes, edges } = metricTreeToFlow(tree, null);
    expect(nodes).toHaveLength(3);
    expect(edges).toHaveLength(2);
    expect(nodes.map((n) => n.id)).toContain("orders.profit");
  });

  it("styles component and driver edges distinctly", () => {
    const { edges } = metricTreeToFlow(tree, null);
    const component = edges.find((e) => e.data?.kind === "component");
    const driver = edges.find((e) => e.data?.kind === "driver");
    expect(component?.className).toBe("metric-edge-component");
    expect(component?.style?.strokeDasharray).toBeUndefined();
    expect(driver?.className).toBe("metric-edge-driver");
    expect(driver?.style?.strokeDasharray).toBe("6 4");
    expect(driver?.animated).toBe(true);
  });

  it("marks the selected node", () => {
    const { nodes } = metricTreeToFlow(tree, "orders.profit");
    const selected = nodes.find((n) => n.id === "orders.profit");
    const other = nodes.find((n) => n.id === "orders.revenue");
    expect((selected?.data as { selected: boolean }).selected).toBe(true);
    expect((other?.data as { selected: boolean }).selected).toBe(false);
  });
});
