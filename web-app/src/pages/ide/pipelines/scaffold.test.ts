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

  it("scaffolds a clickhouse source with a password var, never a raw secret", () => {
    const yaml = buildPipelineScaffold({
      name: "p",
      sourceId: "clickhouse",
      destinationDatabase: "wh",
      datasetName: "p"
    });
    expect(yaml).toContain("kind: clickhouse");
    expect(yaml).toContain("password_var: CLICKHOUSE_PASSWORD");
    expect(yaml).not.toMatch(/^\s*password:/m);
  });

  it("scaffolds a clickhouse source from picked credentials and tables", () => {
    const yaml = buildPipelineScaffold({
      name: "ch_raw",
      sourceId: "clickhouse",
      clickhouse: {
        host: "h.clickhouse.cloud",
        port: 8443,
        database: "analytics",
        username: "reader",
        passwordVar: "CH_PW",
        secure: true,
        tables: [
          { name: "events", writeDisposition: "append", cursorField: "created_at" },
          { name: "users", writeDisposition: "merge", primaryKey: ["id"] },
          { name: "products", writeDisposition: "replace" }
        ]
      },
      destinationDatabase: "wh",
      datasetName: "ch_raw",
      destinationIsAirhouse: true
    });
    expect(yaml).toContain("kind: clickhouse");
    expect(yaml).toContain("host: h.clickhouse.cloud");
    expect(yaml).toContain("password_var: CH_PW");
    // Per-table disposition: append+cursor, merge+key, replace.
    expect(yaml).toContain("- name: events");
    expect(yaml).toContain("cursor_field: created_at");
    expect(yaml).toContain("write_disposition: append");
    expect(yaml).toContain("- name: users");
    expect(yaml).toContain("primary_key:");
    expect(yaml).toContain("- id");
    expect(yaml).toContain("write_disposition: merge");
    expect(yaml).toContain("- name: products");
    expect(yaml).toContain("write_disposition: replace");
    // The raw password is never written into the YAML.
    expect(yaml).not.toMatch(/^\s*password:/m);
    // ClickHouse pipelines default to splitting `a___b` into schema.table.
    expect(yaml).toContain('schema_separator: "___"');
  });

  it("only adds schema_separator for clickhouse sources", () => {
    const yaml = buildPipelineScaffold({
      name: "p",
      sourceId: "postgres_cdc",
      destinationDatabase: "wh",
      datasetName: "p",
      destinationIsAirhouse: true
    });
    expect(yaml).not.toContain("schema_separator");
  });

  it("omits schema_separator for clickhouse into a non-airhouse destination", () => {
    const yaml = buildPipelineScaffold({
      name: "p",
      sourceId: "clickhouse",
      clickhouse: {
        host: "h",
        database: "d",
        passwordVar: "CH_PW",
        tables: [{ name: "events", writeDisposition: "append" }]
      },
      destinationDatabase: "pg_wh",
      datasetName: "p",
      destinationIsAirhouse: false
    });
    expect(yaml).toContain("kind: clickhouse");
    // schema_separator is airhouse-only; a postgres destination rejects it.
    expect(yaml).not.toContain("schema_separator");
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
