// @vitest-environment node

import { describe, expect, it, vi } from "vitest";
import type { MetricTree } from "../metricTree";
import type { AppFetcher } from "./react";
import { createWorldModel, WorldModelScopeUnsupportedError } from "./world-node";

const TREE: MetricTree = {
  root: "orders.net_revenue",
  nodes: [
    {
      id: "orders.net_revenue",
      view: "orders",
      measure: "net_revenue",
      label: "Net Revenue",
      measure_type: "number",
      is_composite: true,
      drillable: true
    },
    {
      id: "orders.gross_revenue",
      view: "orders",
      measure: "gross_revenue",
      label: "Gross Revenue",
      measure_type: "sum",
      is_composite: false,
      drillable: false
    },
    {
      id: "orders.discounts",
      view: "orders",
      measure: "discounts",
      label: "Discounts",
      measure_type: "sum",
      is_composite: false,
      drillable: false
    },
    {
      id: "orders.units",
      view: "orders",
      measure: "units",
      label: "Units",
      measure_type: "sum",
      is_composite: false,
      drillable: false
    }
  ],
  edges: [
    {
      from: "orders.net_revenue",
      to: "orders.gross_revenue",
      kind: "component",
      direction: "positive",
      strength: "strong",
      confidence: "high",
      form: "linear"
    },
    {
      from: "orders.net_revenue",
      to: "orders.discounts",
      kind: "component",
      sign: -1,
      direction: "negative",
      strength: "moderate",
      confidence: "high",
      form: "linear"
    },
    // A grandchild edge (from gross_revenue) — expand("net_revenue") must NOT surface it.
    {
      from: "orders.gross_revenue",
      to: "orders.units",
      kind: "driver",
      direction: "positive",
      strength: "strong",
      confidence: "medium",
      form: "log-log"
    }
  ]
};

interface Call {
  url: string;
  init?: RequestInit;
}

function makeFetcher() {
  const calls: Call[] = [];
  const fetcher = vi.fn(async (url: string, init?: RequestInit) => {
    calls.push({ url, init });
    let payload: unknown = TREE;
    if (url.includes("/sensitivity")) payload = { target: "orders.net_revenue", drivers: [] };
    else if (url.includes("/explain")) payload = { target: "orders.net_revenue", target_delta: 0 };
    else if (url.includes("/opportunity"))
      payload = { target: "orders.net_revenue", dimensions: [] };
    return { ok: true, status: 200, json: async () => payload } as unknown as Response;
  });
  return { fetcher: fetcher as unknown as AppFetcher, calls };
}

describe("createWorldModel", () => {
  it("tree() hits the project-scoped metric-tree endpoint", async () => {
    const { fetcher, calls } = makeFetcher();
    await createWorldModel("proj-1", fetcher).tree();
    expect(calls[0].url).toBe("/api/projects/proj-1/semantic/metric-tree");
  });

  it("tree(root) passes the root as a query param", async () => {
    const { fetcher, calls } = makeFetcher();
    await createWorldModel("proj-1", fetcher).tree("orders.net_revenue");
    expect(calls[0].url).toBe("/api/projects/proj-1/semantic/metric-tree?root=orders.net_revenue");
  });

  it("metric(id).node() returns the matching tree node", async () => {
    const { fetcher } = makeFetcher();
    const node = await createWorldModel("proj-1", fetcher).metric("orders.net_revenue").node();
    expect(node.label).toBe("Net Revenue");
    expect(node.is_composite).toBe(true);
  });

  it("metric(id).node() throws when the measure is absent", async () => {
    const { fetcher } = makeFetcher();
    await expect(
      createWorldModel("proj-1", fetcher).metric("orders.missing").node()
    ).rejects.toThrow(/not found/);
  });

  it("expand() returns only the one-hop children (edge.from === id)", async () => {
    const { fetcher } = makeFetcher();
    const children = await createWorldModel("proj-1", fetcher)
      .metric("orders.net_revenue")
      .expand();
    expect(children.map((c) => c.node.id).sort()).toEqual([
      "orders.discounts",
      "orders.gross_revenue"
    ]);
    // The grandchild reached via gross_revenue must not leak in.
    expect(children.map((c) => c.node.id)).not.toContain("orders.units");
    // Each child carries its edge and a recursable handle.
    const gross = children.find((c) => c.node.id === "orders.gross_revenue");
    expect(gross?.edge.kind).toBe("component");
    expect(gross?.handle.id).toBe("orders.gross_revenue");
  });

  it("drivers() hits the sensitivity endpoint for the measure", async () => {
    const { fetcher, calls } = makeFetcher();
    await createWorldModel("proj-1", fetcher).metric("orders.net_revenue").drivers();
    expect(calls[0].url).toBe(
      "/api/projects/proj-1/semantic/metric-tree/orders.net_revenue/sensitivity"
    );
  });

  it("explain() POSTs to /explain with the target injected and a v:1 envelope", async () => {
    const { fetcher, calls } = makeFetcher();
    await createWorldModel("proj-1", fetcher)
      .metric("orders.net_revenue")
      .explain({
        time_dimension: "orders.order_date",
        current_period: ["2026-06-01", "2026-06-30"],
        previous_period: ["2026-05-01", "2026-05-31"]
      });
    expect(calls[0].url).toBe("/api/projects/proj-1/semantic/metric-tree/explain");
    expect(calls[0].init?.method).toBe("POST");
    expect(JSON.parse(calls[0].init?.body as string)).toMatchObject({
      v: 1,
      target: "orders.net_revenue",
      time_dimension: "orders.order_date"
    });
  });

  it("size() POSTs to /opportunity with the target injected", async () => {
    const { fetcher, calls } = makeFetcher();
    await createWorldModel("proj-1", fetcher)
      .metric("orders.net_revenue")
      .size({
        time_dimension: "orders.order_date",
        period: ["2026-06-01", "2026-06-30"]
      });
    expect(calls[0].url).toBe("/api/projects/proj-1/semantic/metric-tree/opportunity");
    expect(JSON.parse(calls[0].init?.body as string).target).toBe("orders.net_revenue");
  });

  it("drill() accumulates scope and preserves it on expanded children", async () => {
    const { fetcher } = makeFetcher();
    const scoped = createWorldModel("proj-1", fetcher)
      .metric("orders.net_revenue")
      .drill({ region: "west" });
    expect(scoped.scope).toEqual({ region: "west" });

    // Structural verbs stay usable on a drilled handle (scope-invariant),
    // and children inherit the scope.
    const children = await scoped.expand();
    expect(children[0].handle.scope).toEqual({ region: "west" });
  });

  it("value verbs throw WorldModelScopeUnsupportedError on a drilled handle", () => {
    const { fetcher } = makeFetcher();
    const scoped = createWorldModel("proj-1", fetcher)
      .metric("orders.net_revenue")
      .drill({ region: "west" });
    expect(() =>
      scoped.explain({
        time_dimension: "orders.order_date",
        current_period: ["2026-06-01", "2026-06-30"],
        previous_period: ["2026-05-01", "2026-05-31"]
      })
    ).toThrow(WorldModelScopeUnsupportedError);
    expect(() =>
      scoped.size({ time_dimension: "orders.order_date", period: ["2026-06-01", "2026-06-30"] })
    ).toThrow(/not yet supported/);
  });

  it("verbs throw a clear error when no project is active", () => {
    const { fetcher } = makeFetcher();
    expect(() => createWorldModel(null, fetcher).tree()).toThrow(/no active project/);
  });
});
