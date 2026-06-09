// @vitest-environment node

import { describe, expect, it, vi } from "vitest";
import type { OxyConfig } from "./config";
import { MetricTreeClient } from "./metricTree";

function makeClient() {
  const config: OxyConfig = {
    apiKey: "test-key",
    projectId: "proj-123",
    baseUrl: "https://api.test",
    timeout: 5000
  };
  const request = vi.fn(async (_endpoint: string, _options?: RequestInit) => ({}) as unknown);
  return { client: new MetricTreeClient(config, request as never), request, config };
}

describe("MetricTreeClient", () => {
  it("getTree hits the project-scoped tree endpoint", async () => {
    const { client, request } = makeClient();
    await client.getTree();
    expect(request).toHaveBeenCalledWith("/proj-123/semantic/metric-tree");
  });

  it("getTree(root) passes the root as a query param", async () => {
    const { client, request } = makeClient();
    await client.getTree("orders.net_revenue");
    expect(request).toHaveBeenCalledWith("/proj-123/semantic/metric-tree?root=orders.net_revenue");
  });

  it("getSensitivity URL-encodes the measure id", async () => {
    const { client, request } = makeClient();
    await client.getSensitivity("orders.net_revenue");
    expect(request).toHaveBeenCalledWith(
      "/proj-123/semantic/metric-tree/orders.net_revenue/sensitivity"
    );
  });

  it("predict POSTs the changes wrapper", async () => {
    const { client, request } = makeClient();
    await client.predict([{ measure: "marketing_spend.total_spend", delta: 10000 }]);
    expect(request).toHaveBeenCalledWith(
      "/proj-123/semantic/metric-tree/predict",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          changes: [{ measure: "marketing_spend.total_spend", delta: 10000 }]
        })
      })
    );
  });

  it("explain POSTs the request body verbatim", async () => {
    const { client, request } = makeClient();
    const req = {
      target: "financials.operating_profit",
      time_dimension: "financials.month",
      current_period: ["2025-09-01", "2025-09-30"] as [string, string],
      previous_period: ["2025-08-01", "2025-08-31"] as [string, string]
    };
    await client.explain(req);
    expect(request).toHaveBeenCalledWith(
      "/proj-123/semantic/metric-tree/explain",
      expect.objectContaining({ method: "POST", body: JSON.stringify(req) })
    );
  });

  it("findOpportunities POSTs to the opportunity endpoint", async () => {
    const { client, request } = makeClient();
    const req = {
      target: "orders.net_revenue",
      time_dimension: "orders.order_date",
      period: ["2025-09-01", "2025-09-30"] as [string, string]
    };
    await client.findOpportunities(req);
    expect(request).toHaveBeenCalledWith(
      "/proj-123/semantic/metric-tree/opportunity",
      expect.objectContaining({ method: "POST", body: JSON.stringify(req) })
    );
  });

  it("appends branch to the query string when configured", async () => {
    const config: OxyConfig = {
      apiKey: "k",
      projectId: "p",
      baseUrl: "https://api.test",
      branch: "feature/x",
      timeout: 5000
    };
    const request = vi.fn(async () => ({}) as unknown);
    const client = new MetricTreeClient(config, request as never);
    await client.getTree();
    expect(request).toHaveBeenCalledWith("/p/semantic/metric-tree?branch=feature%2Fx");
  });
});
