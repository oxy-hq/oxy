import { describe, expect, it } from "vitest";

import { buildPipelineScaffold } from "./scaffold";

describe("buildPipelineScaffold", () => {
  it("emits a database-reference destination, not raw credentials", () => {
    const yaml = buildPipelineScaffold({
      name: "shopify_raw",
      sourceId: "rest_api",
      destinationDatabase: "my_warehouse",
      datasetName: "shopify_raw"
    });
    expect(yaml).toContain("name: shopify_raw");
    expect(yaml).toContain("kind: rest_api");
    expect(yaml).toContain("database: my_warehouse");
    expect(yaml).toContain("dataset_name: shopify_raw");
    expect(yaml).not.toContain("connection_string"); // never in the dest block
    expect(yaml).toContain("concurrency: 1");
  });

  it("emits a toast source with a secret var, never the raw secret", () => {
    const yaml = buildPipelineScaffold({
      name: "toast_daily",
      sourceId: "toast",
      toast: {
        clientId: "abc123",
        clientSecretVar: "TOAST_PROD_SECRET",
        restaurantGuids: ["11111111-2222-3333-4444-555555555555", "guid-2"],
        baseUrl: "https://sandbox.toasttab.com"
      },
      destinationDatabase: "wh",
      datasetName: "toast_daily"
    });
    expect(yaml).toContain("kind: toast");
    expect(yaml).toContain("client_id: abc123");
    expect(yaml).toContain("client_secret_var: TOAST_PROD_SECRET");
    expect(yaml).toContain('- "11111111-2222-3333-4444-555555555555"');
    expect(yaml).toContain('- "guid-2"');
    expect(yaml).toContain("base_url: https://sandbox.toasttab.com");
    // The secret value itself is never scaffolded.
    expect(yaml).not.toContain("client_secret:");
  });

  it("omits base_url when not provided", () => {
    const yaml = buildPipelineScaffold({
      name: "t",
      sourceId: "toast",
      toast: { clientId: "x", clientSecretVar: "S", restaurantGuids: ["g"] },
      destinationDatabase: "wh",
      datasetName: "t"
    });
    expect(yaml).not.toContain("base_url:");
  });

  it("includes the description line only when provided", () => {
    expect(
      buildPipelineScaffold({
        name: "p",
        sourceId: "filesystem",
        destinationDatabase: "wh",
        datasetName: "p"
      })
    ).not.toContain("description:");

    expect(
      buildPipelineScaffold({
        name: "p",
        description: "hourly ingest",
        sourceId: "filesystem",
        destinationDatabase: "wh",
        datasetName: "p"
      })
    ).toContain('description: "hourly ingest"');
  });

  it("scaffolds the per-source config block", () => {
    expect(
      buildPipelineScaffold({
        name: "p",
        sourceId: "postgres_cdc",
        destinationDatabase: "wh",
        datasetName: "p"
      })
    ).toContain("publication_name: oxy_pub");
  });

  it("falls back to the first option for an unknown source id", () => {
    const yaml = buildPipelineScaffold({
      name: "p",
      sourceId: "nope",
      destinationDatabase: "wh",
      datasetName: "p"
    });
    expect(yaml).toContain("kind: rest_api");
  });
});
