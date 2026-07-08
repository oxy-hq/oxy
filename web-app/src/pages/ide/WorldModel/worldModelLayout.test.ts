import { describe, expect, it } from "vitest";
import type {
  WmBreakdownNode,
  WmComputedMeasure,
  WmMeasureBreakdown,
  WorldModel
} from "@/types/worldModel";
import {
  breakdownNodeToComputedMeasure,
  buildBreakdownEdges,
  buildLayoutSizeMap,
  buildViewToEntityIds,
  composedHandleId,
  composedSelfHandles,
  composedSelfSourceHandleId,
  composedSelfTargetHandleId,
  composedTargetHandles,
  contributorHandleId,
  contributorSourceHandles,
  EXPANDED_NODE_WIDTH,
  groupBreakdownContributorsByEntity,
  layoutSizeForEntity,
  NODE_HEIGHT_COLLAPSED,
  NODE_WIDTH,
  type WmSizingState,
  worldModelToFlow
} from "./worldModelLayout";

function entity(id: string, depth: number): WorldModel["entities"][number] {
  return {
    id,
    label: id,
    view: id,
    depth,
    dimensions: [],
    own_measures: [],
    induced_measures: []
  };
}

const model: WorldModel = {
  entities: [entity("order", 2), entity("customer", 1), entity("store", 3)],
  edges: [
    { from: "order", to: "customer", functional: true },
    { from: "order", to: "store", functional: true }
  ]
};

describe("worldModelToFlow", () => {
  // Regression: React Flow silently drops any edge whose endpoint nodes lack
  // handle bounds (`getEdgePosition` returns null), and our waypoint edge can't
  // self-heal from geometry once dropped. Pre-declared `handles` make nodes
  // edge-ready before DOM measurement, killing the "sometimes edges are
  // missing" race. If a node ever ships without handles, that race returns.
  it("gives every node pre-declared source and target handles", () => {
    const { nodes } = worldModelToFlow(model, null);

    expect(nodes).toHaveLength(3);
    for (const node of nodes) {
      const handles = node.handles ?? [];
      expect(handles.some((h) => h.type === "source")).toBe(true);
      expect(handles.some((h) => h.type === "target")).toBe(true);
    }
  });

  it("keeps every edge endpoint matched to a node id", () => {
    const { nodes, edges } = worldModelToFlow(model, null);
    const ids = new Set(nodes.map((n) => n.id));

    for (const edge of edges) {
      expect(ids.has(edge.source)).toBe(true);
      expect(ids.has(edge.target)).toBe(true);
    }
  });
});

function bnode(
  over: Partial<WmBreakdownNode> & Pick<WmBreakdownNode, "id" | "view" | "measure">
): WmBreakdownNode {
  return {
    label: over.measure,
    measure_type: "sum",
    is_composite: false,
    is_root: false,
    expr: null,
    value: "1",
    unvalued_reason: null,
    ...over
  };
}

describe("buildBreakdownEdges", () => {
  // order.net_revenue = order.total_order_value(+) − order.total_shipping_costs(−)
  // total_order_value rolls up from order_item.total_revenue.
  const breakdown: WmMeasureBreakdown = {
    root: "orders.net_revenue",
    nodes: [
      bnode({
        id: "orders.net_revenue",
        view: "orders",
        measure: "net_revenue",
        is_composite: true,
        is_root: true
      }),
      bnode({
        id: "orders.total_order_value",
        view: "orders",
        measure: "total_order_value",
        is_composite: true
      }),
      bnode({
        id: "orders.total_shipping_costs",
        view: "orders",
        measure: "total_shipping_costs",
        is_composite: true
      }),
      bnode({ id: "order_items.total_revenue", view: "order_items", measure: "total_revenue" }),
      bnode({
        id: "order_shipments.total_shipment_cost",
        view: "order_shipments",
        measure: "total_shipment_cost"
      })
    ],
    edges: [
      { from: "orders.total_order_value", to: "orders.net_revenue", operator: "add", sign: 1 },
      { from: "orders.total_shipping_costs", to: "orders.net_revenue", operator: "sub", sign: -1 },
      {
        from: "order_items.total_revenue",
        to: "orders.total_order_value",
        operator: "add",
        sign: 1
      },
      {
        from: "order_shipments.total_shipment_cost",
        to: "orders.total_shipping_costs",
        operator: "add",
        sign: 1
      }
    ]
  };
  const viewToEntityIds = buildViewToEntityIds([
    {
      id: "order",
      label: "Order",
      view: "orders",
      depth: 2,
      dimensions: [],
      own_measures: [],
      induced_measures: []
    },
    {
      id: "order_item",
      label: "Order Item",
      view: "order_items",
      depth: 3,
      dimensions: [],
      own_measures: [],
      induced_measures: []
    },
    {
      id: "shipment",
      label: "Shipment",
      view: "order_shipments",
      depth: 4,
      dimensions: [],
      own_measures: [],
      induced_measures: []
    }
  ]);

  it("draws an edge per cross-entity contributor, anchored to its measure row", () => {
    const edges = buildBreakdownEdges("order", breakdown, viewToEntityIds);
    // Contributors that live on OTHER cards (order_item, shipment).
    const cross = edges.filter((e) => e.source !== "order");
    expect(cross).toHaveLength(2);

    const fromLineItems = cross.find((e) => e.source === "order_item");
    expect(fromLineItems).toBeDefined();
    expect(fromLineItems?.target).toBe("order");
    expect(fromLineItems?.sourceHandle).toBe(contributorHandleId("total_revenue"));
    // Anchored to the composite it feeds (total_order_value) on the expanded card.
    expect(fromLineItems?.targetHandle).toBe(composedHandleId("total_order_value"));

    const fromShipments = cross.find((e) => e.source === "shipment");
    expect(fromShipments?.sourceHandle).toBe(contributorHandleId("total_shipment_cost"));
    expect(fromShipments?.targetHandle).toBe(composedHandleId("total_shipping_costs"));
  });

  it("draws same-card composition edges between rows on the expanded card", () => {
    const edges = buildBreakdownEdges("order", breakdown, viewToEntityIds);
    // total_order_value and total_shipping_costs both live on the Order card and
    // feed net_revenue there — self-edges on the anchor, on the right gutter.
    const self = edges.filter((e) => e.source === "order" && e.target === "order");
    expect(self).toHaveLength(2);

    const fromOrderValue = self.find(
      (e) => e.sourceHandle === composedSelfSourceHandleId("total_order_value")
    );
    expect(fromOrderValue).toBeDefined();
    expect(fromOrderValue?.targetHandle).toBe(composedSelfTargetHandleId("net_revenue"));
    expect(fromOrderValue?.label).toBe("+");

    const fromShipping = self.find(
      (e) => e.sourceHandle === composedSelfSourceHandleId("total_shipping_costs")
    );
    expect(fromShipping?.targetHandle).toBe(composedSelfTargetHandleId("net_revenue"));
    expect(fromShipping?.label).toBe("−");
  });

  it("never draws an edge originating from the root measure", () => {
    const edges = buildBreakdownEdges("order", breakdown, viewToEntityIds);
    // net_revenue is the root; nothing rolls up FROM it.
    expect(edges.every((e) => e.sourceHandle !== composedSelfSourceHandleId("net_revenue"))).toBe(
      true
    );
  });

  it("returns nothing without a breakdown", () => {
    expect(buildBreakdownEdges("order", null, viewToEntityIds)).toEqual([]);
  });

  // Regression: the breakdown edges above reference per-measure handles
  // (`bkd-src-…` on the contributor card, `bkd-tgt-…` on the expanded card).
  // Those handles are rendered conditionally, so React Flow drops the edge if
  // it commits before the card is measured — the "sometimes the contributor→
  // metric edge is missing" race. Pre-declaring the handles on the node closes
  // it, but ONLY if the declared set covers exactly the ids the edges point at.
  // This asserts that contract at the pure-function level.
  it("pre-declares handles covering every breakdown edge endpoint", () => {
    const edges = buildBreakdownEdges("order", breakdown, viewToEntityIds);
    const contributorsByEntity = groupBreakdownContributorsByEntity(breakdown, viewToEntityIds);

    // The expanded card shows the root plus its own contributors; its declared
    // handles cover both left targets (cross-entity) and right source+target
    // (same-card).
    const rootNode = breakdown.nodes.find((n) => n.id === breakdown.root);
    if (!rootNode) throw new Error("breakdown root not found");
    const expandedMeasures = [
      breakdownNodeToComputedMeasure(rootNode),
      ...(contributorsByEntity.get("order") ?? [])
    ];
    const expandedHandleIds = new Set(
      [...composedTargetHandles(expandedMeasures), ...composedSelfHandles(expandedMeasures)].map(
        (h) => h.id
      )
    );

    for (const edge of edges) {
      // Source: a same-card edge sources from the anchor (self handles); a
      // cross-entity edge sources from its own contributor card.
      const sourceHandleIds =
        edge.source === "order"
          ? expandedHandleIds
          : new Set(
              contributorSourceHandles(contributorsByEntity.get(edge.source) ?? []).map((h) => h.id)
            );
      expect(sourceHandleIds.has(edge.sourceHandle as string)).toBe(true);
      // Target is always a row on the expanded card.
      expect(expandedHandleIds.has(edge.targetHandle as string)).toBe(true);
    }
  });
});

describe("layoutSizeForEntity", () => {
  const measure = (name: string): WmComputedMeasure => ({
    name,
    measure_type: "sum",
    value: "1",
    fiber_count: 0,
    label: name
  });

  const baseState: WmSizingState = {
    expandedEntityId: null,
    expandedRowCount: null,
    filterSeedEntityId: null,
    seedComputedMeasures: null,
    contributorsByEntity: new Map(),
    filterCounts: null,
    isCountLoading: false
  };

  it("uses the collapsed box for an idle card", () => {
    expect(layoutSizeForEntity("order", baseState)).toEqual({
      width: NODE_WIDTH,
      height: NODE_HEIGHT_COLLAPSED
    });
  });

  it("grows the expanded card to the wide box and reserves a row per measure", () => {
    const two = layoutSizeForEntity("order", {
      ...baseState,
      expandedEntityId: "order",
      expandedRowCount: 2
    });
    const four = layoutSizeForEntity("order", {
      ...baseState,
      expandedEntityId: "order",
      expandedRowCount: 4
    });
    expect(two.width).toBe(EXPANDED_NODE_WIDTH);
    expect(four.width).toBe(EXPANDED_NODE_WIDTH);
    // More rows ⇒ a taller reserved box, so neighbors are pushed further.
    expect(four.height).toBeGreaterThan(two.height);
  });

  it("reserves a smaller placeholder box while the breakdown is still loading", () => {
    const loading = layoutSizeForEntity("order", {
      ...baseState,
      expandedEntityId: "order",
      expandedRowCount: null
    });
    const loaded = layoutSizeForEntity("order", {
      ...baseState,
      expandedEntityId: "order",
      expandedRowCount: 3
    });
    expect(loading.height).toBeLessThan(loaded.height);
  });

  it("grows the filter-seed card to fit its measure chips", () => {
    const size = layoutSizeForEntity("order", {
      ...baseState,
      filterSeedEntityId: "order",
      seedComputedMeasures: [measure("revenue"), measure("orders"), measure("aov")]
    });
    expect(size.width).toBe(NODE_WIDTH);
    expect(size.height).toBeGreaterThan(NODE_HEIGHT_COLLAPSED);
  });

  it("grows a card with descendant sample chips, and adds room for the 'more' button", () => {
    const withMore = layoutSizeForEntity("customer", {
      ...baseState,
      filterCounts: {
        customer: { matched: 20, total: 100, sample: ["a", "b", "c"] }
      }
    });
    const exact = layoutSizeForEntity("customer", {
      ...baseState,
      filterCounts: {
        customer: { matched: 3, total: 100, sample: ["a", "b", "c"] }
      }
    });
    // matched (20) > shown samples (3) ⇒ a "+N more" row, so it's taller.
    expect(withMore.height).toBeGreaterThan(exact.height);
  });

  it("prefers a breakdown contributor's chips over the seed's own on the same card", () => {
    const contributorsByEntity = new Map([["order", [measure("a"), measure("b")]]]);
    const size = layoutSizeForEntity("order", {
      ...baseState,
      filterSeedEntityId: "order",
      seedComputedMeasures: [measure("x")],
      contributorsByEntity
    });
    // Sized for the 2 contributor chips, not the single seed chip.
    const twoChip = layoutSizeForEntity("order", {
      ...baseState,
      filterSeedEntityId: "order",
      seedComputedMeasures: [measure("a"), measure("b")]
    });
    expect(size.height).toBe(twoChip.height);
  });
});

describe("buildLayoutSizeMap", () => {
  it("sizes every requested entity, defaulting untouched ones to collapsed", () => {
    const map = buildLayoutSizeMap(["order", "customer"], {
      expandedEntityId: "order",
      expandedRowCount: 2,
      filterSeedEntityId: null,
      seedComputedMeasures: null,
      contributorsByEntity: new Map(),
      filterCounts: null,
      isCountLoading: false
    });
    expect(map.get("customer")).toEqual({
      width: NODE_WIDTH,
      height: NODE_HEIGHT_COLLAPSED
    });
    expect(map.get("order")?.width).toBe(EXPANDED_NODE_WIDTH);
  });
});
